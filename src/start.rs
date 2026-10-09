use std::io::{self, Write};
use std::path::Path;

use crate::doctor;
use crate::runner::CommandRunner;
use crate::scrcpy::{self, Options};

/// 檢查不通過或 scrcpy 無法啟動、被訊號中止時的 exit code。
const FAILURE: u8 = 1;

/// 先跑和 `doctor` 相同的檢查，全部就緒才以 DeX 虛擬顯示模式啟動 scrcpy，並等它結束（會阻塞到視窗關閉）。
///
/// 回傳 dex2pc 應該使用的 exit code：scrcpy 正常結束為 0；scrcpy 非 0 結束時沿用它的 code，
/// 並印出它的錯誤訊息；檢查不通過（缺工具、選不到裝置、不是 One UI 8 以上的 Samsung）時
/// 回傳 1 且不啟動 scrcpy。
///
/// # Errors
/// 只有寫入 `out` 失敗時回傳錯誤。
pub fn run(
    runner: &dyn CommandRunner,
    options: &Options,
    serial: Option<&str>,
    sysfs_root: &Path,
    out: &mut dyn Write,
) -> io::Result<u8> {
    let Some(selected) = doctor::check(runner, serial, sysfs_root, out)? else {
        writeln!(out, "環境未就緒，未啟動 scrcpy；請依上面的訊息處理後再執行")?;
        return Ok(FAILURE);
    };
    let args = scrcpy::build_args(options, Some(&selected));
    writeln!(out, "啟動：{}", command_line(&args))?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    // scrcpy 的輸出被收集起來，結束後才看得到；失敗時才印，正常關閉視窗時不印
    let output = match runner.run("scrcpy", &arg_refs) {
        Ok(output) => output,
        Err(err) => {
            writeln!(out, "無法執行 scrcpy：{err}")?;
            return Ok(FAILURE);
        }
    };
    match output.code {
        Some(0) => Ok(0),
        Some(code) => {
            let message = if output.stderr.trim().is_empty() {
                output.stdout.trim()
            } else {
                output.stderr.trim()
            };
            writeln!(out, "scrcpy 以 exit code {code} 結束：\n{message}")?;
            Ok(u8::try_from(code).unwrap_or(FAILURE))
        }
        None => {
            writeln!(out, "scrcpy 被訊號中止")?;
            Ok(FAILURE)
        }
    }
}

/// `--dry-run`：只印出會執行的完整 scrcpy 指令，不呼叫任何外部程式（所以也不檢查環境、不選裝置，
/// 沒給 `serial` 時指令裡就沒有 `--serial`）。
///
/// # Errors
/// 只有寫入 `out` 失敗時回傳錯誤。
pub fn dry_run(options: &Options, serial: Option<&str>, out: &mut dyn Write) -> io::Result<()> {
    writeln!(
        out,
        "{}",
        command_line(&scrcpy::build_args(options, serial))
    )
}

/// 把 `scrcpy` 與參數組成可以直接貼到 shell 執行的一行；含空白等特殊字元的參數加單引號。
fn command_line(args: &[String]) -> String {
    let mut line = String::from("scrcpy");
    for arg in args {
        line.push(' ');
        line.push_str(&shell_quote(arg));
    }
    line
}

fn shell_quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_=./:,+@%".contains(c));
    if plain {
        arg.to_string()
    } else {
        // 單引號內沒有跳脫字元，參數本身的 ' 要先結束引號、補 \'、再開引號
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io::ErrorKind;

    use tempfile::TempDir;

    use super::*;
    use crate::runner::CommandOutput;

    const ADB_OUTPUT: &str = "Android Debug Bridge version 1.0.41\n";
    const SCRCPY_OUTPUT: &str = "scrcpy 3.3.1 <https://github.com/Genymobile/scrcpy>\n";
    const ONE_DEVICE: &str = "List of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\n\n";
    const S23_ONE_UI_80: &str = "[ro.build.version.oneui]: [80000]\n[ro.build.version.sdk]: [36]\n[ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-S918B]\n";
    const S23_ONE_UI_71: &str = "[ro.build.version.oneui]: [70100]\n[ro.build.version.sdk]: [35]\n[ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-S918B]\n";
    const PIXEL: &str = "[ro.build.version.sdk]: [36]\n[ro.product.manufacturer]: [Google]\n[ro.product.model]: [Pixel 9]\n";

    /// 環境檢查的指令一律成功；getprop 內容與啟動 scrcpy 的結果由測試指定。
    /// 其他指令直接 panic，所有呼叫依序記在 `calls`。
    struct FakeRunner {
        getprop: &'static str,
        launch: fn() -> io::Result<CommandOutput>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeRunner {
        fn new(getprop: &'static str, launch: fn() -> io::Result<CommandOutput>) -> Self {
            Self {
                getprop,
                launch,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    fn finished(code: Option<i32>, stdout: &str, stderr: &str) -> CommandOutput {
        CommandOutput {
            code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            match (program, args) {
                ("adb", ["version"]) => Ok(finished(Some(0), ADB_OUTPUT, "")),
                ("scrcpy", ["--version"]) => Ok(finished(Some(0), SCRCPY_OUTPUT, "")),
                ("adb", ["devices", "-l"]) => Ok(finished(Some(0), ONE_DEVICE, "")),
                ("adb", ["-s", "R5CW1234ABC", "shell", "getprop"]) => {
                    Ok(finished(Some(0), self.getprop, ""))
                }
                ("scrcpy", _) => (self.launch)(),
                _ => panic!("unexpected command: {program} {args:?}"),
            }
        }
    }

    fn start(runner: &FakeRunner, options: &Options) -> (u8, String) {
        let sysfs = TempDir::new().unwrap();
        let mut out = Vec::new();
        let code = run(runner, options, None, sysfs.path(), &mut out).unwrap();
        (code, String::from_utf8(out).unwrap())
    }

    #[test]
    fn start_on_supported_device_launches_scrcpy_on_dex_display_last() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || Ok(finished(Some(0), "", "")));

        start(&runner, &Options::default());

        assert_eq!(
            *runner.calls.borrow(),
            vec![
                "adb version".to_string(),
                "scrcpy --version".to_string(),
                "adb devices -l".to_string(),
                "adb -s R5CW1234ABC shell getprop".to_string(),
                "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 --window-title=Samsung DeX"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn start_passes_configured_options_to_scrcpy() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || Ok(finished(Some(0), "", "")));
        let options = Options {
            video_bit_rate: Some("16M".to_string()),
            audio: false,
            ..Options::default()
        };

        start(&runner, &options);

        assert_eq!(
            runner.calls.borrow().last().unwrap(),
            "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 --window-title=Samsung DeX --video-bit-rate=16M --no-audio"
        );
    }

    #[test]
    fn start_when_scrcpy_exits_normally_returns_zero() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || Ok(finished(Some(0), "", "")));

        let (code, _) = start(&runner, &Options::default());

        assert_eq!(code, 0);
    }

    #[test]
    fn start_prints_scrcpy_command_before_launching() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || Ok(finished(Some(0), "", "")));

        let (_, out) = start(&runner, &Options::default());

        assert!(out.ends_with(
            "啟動：scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 '--window-title=Samsung DeX'\n"
        ));
    }

    #[test]
    fn start_on_one_ui_7_fails_without_launching_scrcpy() {
        let runner = FakeRunner::new(S23_ONE_UI_71, || panic!("scrcpy must not be launched"));

        let (code, out) = start(&runner, &Options::default());

        assert_eq!(code, 1);
        assert!(out.ends_with("環境未就緒，未啟動 scrcpy；請依上面的訊息處理後再執行\n"));
    }

    #[test]
    fn start_on_non_samsung_device_fails_without_launching_scrcpy() {
        let runner = FakeRunner::new(PIXEL, || panic!("scrcpy must not be launched"));

        let (code, _) = start(&runner, &Options::default());

        assert_eq!(code, 1);
    }

    #[test]
    fn start_when_scrcpy_fails_returns_its_exit_code() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || {
            Ok(finished(Some(2), "", "ERROR: Could not create display\n"))
        });

        let (code, _) = start(&runner, &Options::default());

        assert_eq!(code, 2);
    }

    #[test]
    fn start_when_scrcpy_fails_prints_its_error_message() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || {
            Ok(finished(Some(2), "", "ERROR: Could not create display\n"))
        });

        let (_, out) = start(&runner, &Options::default());

        assert!(out.ends_with("scrcpy 以 exit code 2 結束：\nERROR: Could not create display\n"));
    }

    #[test]
    fn start_when_scrcpy_fails_with_empty_stderr_prints_stdout() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || {
            Ok(finished(Some(1), "ERROR: device gone\n", ""))
        });

        let (_, out) = start(&runner, &Options::default());

        assert!(out.ends_with("scrcpy 以 exit code 1 結束：\nERROR: device gone\n"));
    }

    #[test]
    fn start_when_scrcpy_killed_by_signal_returns_one() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || Ok(finished(None, "", "")));

        let (code, out) = start(&runner, &Options::default());

        assert_eq!(code, 1);
        assert!(out.ends_with("scrcpy 被訊號中止\n"));
    }

    #[test]
    fn start_when_scrcpy_cannot_be_executed_returns_one() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || {
            Err(io::Error::from(ErrorKind::PermissionDenied))
        });

        let (code, _) = start(&runner, &Options::default());

        assert_eq!(code, 1);
    }

    #[test]
    fn start_with_serial_launches_scrcpy_on_that_device() {
        let runner = FakeRunner::new(S23_ONE_UI_80, || Ok(finished(Some(0), "", "")));
        let sysfs = TempDir::new().unwrap();

        run(
            &runner,
            &Options::default(),
            Some("R5CW1234ABC"),
            sysfs.path(),
            &mut Vec::new(),
        )
        .unwrap();

        assert!(
            runner
                .calls
                .borrow()
                .last()
                .unwrap()
                .starts_with("scrcpy --serial=R5CW1234ABC ")
        );
    }

    #[test]
    fn dry_run_prints_full_command_with_quoted_title() {
        let mut out = Vec::new();

        dry_run(&Options::default(), None, &mut out).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "scrcpy --new-display=1920x1080/160 '--window-title=Samsung DeX'\n"
        );
    }

    #[test]
    fn dry_run_with_serial_includes_serial() {
        let mut out = Vec::new();

        dry_run(&Options::default(), Some("R5CW1234ABC"), &mut out).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 '--window-title=Samsung DeX'\n"
        );
    }

    #[test]
    fn shell_quote_escapes_single_quote() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn shell_quote_wraps_empty_argument() {
        assert_eq!(shell_quote(""), "''");
    }
}
