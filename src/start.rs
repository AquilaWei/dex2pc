use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::doctor;
use crate::runner::CommandRunner;
use crate::scrcpy::{self, Options};

/// 檢查不通過或 scrcpy 無法啟動、被訊號中止時的 exit code。
const FAILURE: u8 = 1;

/// scrcpy 在連線建立後裝置才斷開時的 exit code（見 scrcpy man page 的 Exit status）。
const SCRCPY_DISCONNECTED: i32 = 2;

/// `--watch` 的重連策略。
pub struct Watch {
    /// 斷線後連續幾次重新啟動失敗就放棄；成功連上（之後又斷線）會把計數歸零。
    pub max_failures: u32,
    /// 每次重新啟動 scrcpy 前等待的時間，讓剛接回的手機有時間就緒；測試設為 0。
    pub retry_delay: Duration,
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            max_failures: 5,
            retry_delay: Duration::from_secs(2),
        }
    }
}

/// 一次執行 scrcpy 的結果。
enum Outcome {
    /// 使用者關閉視窗（exit code 0）。
    Closed,
    /// 連線中途斷開（exit code 2），watch 模式會重連。
    Disconnected,
    /// 其他失敗；內容是 dex2pc 應使用的 exit code。
    Failed(u8),
    /// 被訊號中止（例如 Ctrl+C）。
    Interrupted,
}

impl Outcome {
    fn exit_code(&self) -> u8 {
        match self {
            Outcome::Closed => 0,
            Outcome::Disconnected => 2,
            Outcome::Failed(code) => *code,
            Outcome::Interrupted => FAILURE,
        }
    }
}

/// 先跑和 `doctor` 相同的檢查，全部就緒才以 DeX 虛擬顯示模式啟動 scrcpy，並等它結束（會阻塞到視窗關閉）。
///
/// 回傳 dex2pc 應該使用的 exit code：scrcpy 正常結束為 0；scrcpy 非 0 結束時沿用它的 code，
/// 並印出它的錯誤訊息；檢查不通過（缺工具、選不到裝置、不是 One UI 8 以上的 Samsung）時
/// 回傳 1 且不啟動 scrcpy。
///
/// 給了 `watch` 時，scrcpy 因裝置斷線結束（exit code 2）會等裝置重新出現後自動重新啟動，
/// 直到使用者關閉視窗、按 Ctrl+C，或斷線後連續重新啟動失敗達上限（回傳 1）。
/// 第一次啟動就失敗不重試，因為那通常是設定或環境問題，重試也不會好。
///
/// # Errors
/// 只有寫入 `out` 失敗時回傳錯誤。
pub fn run(
    runner: &dyn CommandRunner,
    options: &Options,
    serial: Option<&str>,
    watch: Option<&Watch>,
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
    let outcome = launch(runner, &arg_refs, out)?;
    match watch {
        Some(watch) => keep_reconnecting(
            runner,
            &arg_refs,
            &selected,
            watch,
            outcome,
            &mut thread::sleep,
            out,
        ),
        None => Ok(outcome.exit_code()),
    }
}

/// watch 模式的主迴圈：從第一次啟動的結果開始，斷線就等裝置回來再重新啟動。
/// `sleep` 只為了讓測試記錄等待時間而不真的等，正式執行傳 `thread::sleep`。
fn keep_reconnecting(
    runner: &dyn CommandRunner,
    args: &[&str],
    serial: &str,
    watch: &Watch,
    first: Outcome,
    sleep: &mut dyn FnMut(Duration),
    out: &mut dyn Write,
) -> io::Result<u8> {
    if !matches!(first, Outcome::Disconnected) {
        return Ok(first.exit_code());
    }
    let mut outcome = first;
    let mut failures = 0;
    loop {
        match outcome {
            Outcome::Closed => return Ok(0),
            Outcome::Interrupted => return Ok(FAILURE),
            Outcome::Disconnected => {
                failures = 0;
                writeln!(out, "裝置已斷線，等待重新連接（按 Ctrl+C 結束）")?;
            }
            Outcome::Failed(_) => {
                failures += 1;
                if failures >= watch.max_failures {
                    writeln!(out, "連續 {failures} 次重新啟動失敗，放棄重連")?;
                    return Ok(FAILURE);
                }
                writeln!(
                    out,
                    "重新啟動失敗（第 {failures}/{} 次），稍後再試",
                    watch.max_failures
                )?;
                // adb wait-for-device 失敗時會立刻返回，不等一下的話暫時性故障會瞬間耗盡重試次數
                sleep(watch.retry_delay);
            }
        }
        outcome = reconnect(runner, args, serial, watch.retry_delay, sleep, out)?;
    }
}

/// 等 `serial` 重新出現（`adb wait-for-device` 會阻塞到裝置就緒），等 `delay` 後重新啟動 scrcpy。
fn reconnect(
    runner: &dyn CommandRunner,
    args: &[&str],
    serial: &str,
    delay: Duration,
    sleep: &mut dyn FnMut(Duration),
    out: &mut dyn Write,
) -> io::Result<Outcome> {
    match runner.run("adb", &["-s", serial, "wait-for-device"]) {
        Ok(output) if output.success() => {}
        // Ctrl+C 也會送到 adb，此時不再重試
        Ok(output) if output.code.is_none() => return Ok(Outcome::Interrupted),
        Ok(output) => {
            writeln!(out, "adb wait-for-device 失敗：{}", output.stderr.trim())?;
            return Ok(Outcome::Failed(FAILURE));
        }
        Err(err) => {
            writeln!(out, "無法執行 adb：{err}")?;
            return Ok(Outcome::Failed(FAILURE));
        }
    }
    sleep(delay);
    writeln!(out, "裝置已重新連接，重新啟動 scrcpy")?;
    launch(runner, args, out)
}

/// 執行一次 scrcpy 並等它結束；失敗時印出原因。
fn launch(runner: &dyn CommandRunner, args: &[&str], out: &mut dyn Write) -> io::Result<Outcome> {
    // scrcpy 的輸出被收集起來，結束後才看得到；失敗時才印，正常關閉視窗時不印
    let output = match runner.run("scrcpy", args) {
        Ok(output) => output,
        Err(err) => {
            writeln!(out, "無法執行 scrcpy：{err}")?;
            return Ok(Outcome::Failed(FAILURE));
        }
    };
    match output.code {
        Some(0) => Ok(Outcome::Closed),
        Some(code) => {
            let message = if output.stderr.trim().is_empty() {
                output.stdout.trim()
            } else {
                output.stderr.trim()
            };
            writeln!(out, "scrcpy 以 exit code {code} 結束：\n{message}")?;
            if code == SCRCPY_DISCONNECTED {
                Ok(Outcome::Disconnected)
            } else {
                Ok(Outcome::Failed(u8::try_from(code).unwrap_or(FAILURE)))
            }
        }
        None => {
            writeln!(out, "scrcpy 被訊號中止")?;
            Ok(Outcome::Interrupted)
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
    use std::collections::VecDeque;
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
        let code = run(runner, options, None, None, sysfs.path(), &mut out).unwrap();
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
                "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 --window-title=Samsung DeX --keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local"
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
            "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 --window-title=Samsung DeX --video-bit-rate=16M --no-audio --keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local"
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
            "啟動：scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 '--window-title=Samsung DeX' --keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local\n"
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
            None,
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

    /// watch 模式用：環境檢查一律通過，scrcpy 與 `adb wait-for-device` 依序回傳 `launches`、`waits`
    /// 裡的結果（None 代表被訊號中止），用完還被呼叫就 panic。所有呼叫依序記在 `calls`。
    struct WatchRunner {
        launches: RefCell<VecDeque<Option<i32>>>,
        waits: RefCell<VecDeque<Option<i32>>>,
        calls: RefCell<Vec<String>>,
    }

    impl WatchRunner {
        fn new(launches: &[Option<i32>], waits: &[Option<i32>]) -> Self {
            Self {
                launches: RefCell::new(launches.iter().copied().collect()),
                waits: RefCell::new(waits.iter().copied().collect()),
                calls: RefCell::new(Vec::new()),
            }
        }

        /// 只留 scrcpy 啟動與 wait-for-device，略過開頭的環境檢查。
        fn session_calls(&self) -> Vec<String> {
            self.calls.borrow()[4..].to_vec()
        }
    }

    impl CommandRunner for WatchRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            match (program, args) {
                ("adb", ["version"]) => Ok(finished(Some(0), ADB_OUTPUT, "")),
                ("scrcpy", ["--version"]) => Ok(finished(Some(0), SCRCPY_OUTPUT, "")),
                ("adb", ["devices", "-l"]) => Ok(finished(Some(0), ONE_DEVICE, "")),
                ("adb", ["-s", "R5CW1234ABC", "shell", "getprop"]) => {
                    Ok(finished(Some(0), S23_ONE_UI_80, ""))
                }
                ("adb", ["-s", "R5CW1234ABC", "wait-for-device"]) => {
                    let code = self
                        .waits
                        .borrow_mut()
                        .pop_front()
                        .expect("unexpected wait");
                    Ok(finished(code, "", ""))
                }
                ("scrcpy", _) => {
                    let code = self
                        .launches
                        .borrow_mut()
                        .pop_front()
                        .expect("unexpected launch");
                    Ok(finished(code, "", "ERROR: device disconnected\n"))
                }
                _ => panic!("unexpected command: {program} {args:?}"),
            }
        }
    }

    const LAUNCH: &str = "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 --window-title=Samsung DeX --keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local";
    const WAIT: &str = "adb -s R5CW1234ABC wait-for-device";

    fn watch(runner: &WatchRunner) -> (u8, String) {
        let sysfs = TempDir::new().unwrap();
        let mut out = Vec::new();
        let policy = Watch {
            max_failures: 5,
            retry_delay: Duration::ZERO,
        };
        let code = run(
            runner,
            &Options::default(),
            None,
            Some(&policy),
            sysfs.path(),
            &mut out,
        )
        .unwrap();
        (code, String::from_utf8(out).unwrap())
    }

    #[test]
    fn watch_after_disconnect_waits_for_device_then_relaunches() {
        let runner = WatchRunner::new(&[Some(2), Some(0)], &[Some(0)]);

        watch(&runner);

        assert_eq!(runner.session_calls(), vec![LAUNCH, WAIT, LAUNCH]);
    }

    #[test]
    fn watch_after_reconnect_and_window_closed_returns_zero() {
        let runner = WatchRunner::new(&[Some(2), Some(0)], &[Some(0)]);

        let (code, _) = watch(&runner);

        assert_eq!(code, 0);
    }

    #[test]
    fn watch_when_window_closed_normally_exits_without_reconnecting() {
        let runner = WatchRunner::new(&[Some(0)], &[]);

        let (code, _) = watch(&runner);

        assert_eq!(code, 0);
        assert_eq!(runner.session_calls(), vec![LAUNCH]);
    }

    #[test]
    fn watch_when_first_launch_fails_does_not_retry() {
        let runner = WatchRunner::new(&[Some(1)], &[]);

        let (code, _) = watch(&runner);

        assert_eq!(code, 1);
        assert_eq!(runner.session_calls(), vec![LAUNCH]);
    }

    #[test]
    fn watch_gives_up_after_five_consecutive_failed_relaunches() {
        let runner = WatchRunner::new(
            &[Some(2), Some(1), Some(1), Some(1), Some(1), Some(1)],
            &[Some(0), Some(0), Some(0), Some(0), Some(0)],
        );

        let (code, out) = watch(&runner);

        assert_eq!(code, 1);
        assert!(out.ends_with("連續 5 次重新啟動失敗，放棄重連\n"));
    }

    #[test]
    fn watch_counts_failed_wait_for_device_toward_limit() {
        let runner = WatchRunner::new(&[Some(2)], &[Some(1), Some(1), Some(1), Some(1), Some(1)]);

        let (code, _) = watch(&runner);

        assert_eq!(code, 1);
        assert_eq!(
            runner.session_calls(),
            vec![LAUNCH, WAIT, WAIT, WAIT, WAIT, WAIT]
        );
    }

    #[test]
    fn watch_waits_retry_delay_after_each_failed_wait_for_device_before_giving_up() {
        let runner = WatchRunner::new(&[], &[Some(1), Some(1), Some(1), Some(1), Some(1)]);
        let policy = Watch {
            max_failures: 5,
            retry_delay: Duration::from_secs(2),
        };
        let mut sleeps = Vec::new();

        keep_reconnecting(
            &runner,
            &[],
            "R5CW1234ABC",
            &policy,
            Outcome::Disconnected,
            &mut |delay| sleeps.push(delay),
            &mut Vec::new(),
        )
        .unwrap();

        assert_eq!(sleeps, vec![Duration::from_secs(2); 4]);
    }

    #[test]
    fn watch_resets_failure_count_after_successful_reconnect() {
        // 4 次失敗後連上又斷線，計數歸零，所以再失敗 4 次仍不放棄，最後正常關閉
        let runner = WatchRunner::new(
            &[
                Some(2),
                Some(1),
                Some(1),
                Some(1),
                Some(1),
                Some(2),
                Some(1),
                Some(1),
                Some(1),
                Some(1),
                Some(0),
            ],
            &[Some(0); 10],
        );

        let (code, _) = watch(&runner);

        assert_eq!(code, 0);
    }

    #[test]
    fn watch_when_scrcpy_interrupted_stops_without_reconnecting() {
        let runner = WatchRunner::new(&[Some(2), None], &[Some(0)]);

        let (code, _) = watch(&runner);

        assert_eq!(code, 1);
        assert_eq!(runner.session_calls(), vec![LAUNCH, WAIT, LAUNCH]);
    }

    #[test]
    fn watch_when_wait_for_device_interrupted_stops() {
        let runner = WatchRunner::new(&[Some(2)], &[None]);

        let (code, _) = watch(&runner);

        assert_eq!(code, 1);
        assert_eq!(runner.session_calls(), vec![LAUNCH, WAIT]);
    }

    #[test]
    fn watch_prints_reconnect_progress() {
        let runner = WatchRunner::new(&[Some(2), Some(0)], &[Some(0)]);

        let (_, out) = watch(&runner);

        assert!(out.ends_with(
            "scrcpy 以 exit code 2 結束：\nERROR: device disconnected\n\
             裝置已斷線，等待重新連接（按 Ctrl+C 結束）\n\
             裝置已重新連接，重新啟動 scrcpy\n"
        ));
    }

    #[test]
    fn start_without_watch_on_disconnect_returns_two_without_reconnecting() {
        let runner = WatchRunner::new(&[Some(2)], &[]);
        let sysfs = TempDir::new().unwrap();

        let code = run(
            &runner,
            &Options::default(),
            None,
            None,
            sysfs.path(),
            &mut Vec::new(),
        )
        .unwrap();

        assert_eq!(code, 2);
        assert_eq!(runner.session_calls(), vec![LAUNCH]);
    }

    #[test]
    fn dry_run_prints_full_command_with_quoted_title() {
        let mut out = Vec::new();

        dry_run(&Options::default(), None, &mut out).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "scrcpy --new-display=1920x1080/160 '--window-title=Samsung DeX' --keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local\n"
        );
    }

    #[test]
    fn dry_run_with_serial_includes_serial() {
        let mut out = Vec::new();

        dry_run(&Options::default(), Some("R5CW1234ABC"), &mut out).unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 '--window-title=Samsung DeX' --keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local\n"
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
