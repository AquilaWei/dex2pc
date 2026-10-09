use std::fmt;
use std::io::{self, ErrorKind, Write};

use crate::device;
use crate::runner::CommandRunner;

pub const ADB_INSTALL_HINT: &str = "sudo dnf install android-tools";
// scrcpy 官方 doc/linux.md 列的 Fedora 來源是 COPR zeno/scrcpy（2026-10 查證）
pub const SCRCPY_INSTALL_HINT: &str = "sudo dnf copr enable zeno/scrcpy && sudo dnf install scrcpy";

/// `--new-display`（在手機上建立虛擬顯示器）從 scrcpy 3.0 開始才有。
pub const MIN_SCRCPY: Version = Version(3, 0, 0);

/// 主、次、修訂版號；欄位順序讓 derive 出來的比較就是版本大小的比較。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// 從 `scrcpy --version` 的第一行（例如 `scrcpy 3.3.1 <https://...>`）取出版號。
///
/// 缺少的次版號或修訂號視為 0（`scrcpy 3.0` → 3.0.0）。
/// 第一行不是 `scrcpy <版號>` 或版號不是數字時回傳 `None`。
pub fn parse_scrcpy_version(output: &str) -> Option<Version> {
    let mut words = output.lines().next()?.split_whitespace();
    if words.next()? != "scrcpy" {
        return None;
    }
    let mut parts = words.next()?.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    Some(Version(major, minor, patch))
}

/// 從 `adb version` 的第一行（`Android Debug Bridge version 1.0.41`）取出版號字串。
pub fn parse_adb_version(output: &str) -> Option<&str> {
    output
        .lines()
        .next()?
        .strip_prefix("Android Debug Bridge version ")
        .map(str::trim)
}

/// 檢查電腦端的 adb 與 scrcpy，以及連接的手機，把結果寫到 `out`。
///
/// 工具兩項都會檢查完才結束，讓使用者一次看到所有問題；adb 可用時再選裝置
/// （`serial` 為 `None` 時自動選唯一一台可用的）。回傳 `Ok(true)` 表示全部就緒。
///
/// # Errors
/// 只有寫入 `out` 失敗時回傳錯誤；工具缺少、版本不符或選不到裝置是回傳 `Ok(false)`。
pub fn run(
    runner: &dyn CommandRunner,
    serial: Option<&str>,
    out: &mut dyn Write,
) -> io::Result<bool> {
    let adb_ok = check_adb(runner, out)?;
    let scrcpy_ok = check_scrcpy(runner, out)?;
    // adb 不能用就列不出裝置，前面已經報過錯，不重複
    let device_ok = adb_ok && check_device(runner, serial, out)?;
    Ok(adb_ok && scrcpy_ok && device_ok)
}

fn check_device(
    runner: &dyn CommandRunner,
    serial: Option<&str>,
    out: &mut dyn Write,
) -> io::Result<bool> {
    let output = match runner.run("adb", &["devices", "-l"]) {
        Ok(output) if output.success() => output,
        Ok(output) => {
            writeln!(out, "adb devices 執行失敗：{}", output.stderr.trim())?;
            return Ok(false);
        }
        Err(err) => {
            writeln!(out, "無法執行 adb devices：{err}")?;
            return Ok(false);
        }
    };
    match device::select(&device::parse_devices(&output.stdout), serial) {
        Ok(device) => {
            writeln!(out, "裝置：{device}")?;
            Ok(true)
        }
        Err(err) => {
            writeln!(out, "{err}")?;
            Ok(false)
        }
    }
}

fn check_adb(runner: &dyn CommandRunner, out: &mut dyn Write) -> io::Result<bool> {
    match runner.run("adb", &["version"]) {
        Ok(output) if output.success() => {
            let version = parse_adb_version(&output.stdout).unwrap_or("無法判斷版本");
            writeln!(out, "adb：{version}")?;
            Ok(true)
        }
        Ok(output) => {
            writeln!(out, "adb 執行失敗：{}", output.stderr.trim())?;
            Ok(false)
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            writeln!(out, "缺少 adb，請安裝：{ADB_INSTALL_HINT}")?;
            Ok(false)
        }
        Err(err) => {
            writeln!(out, "無法執行 adb：{err}")?;
            Ok(false)
        }
    }
}

fn check_scrcpy(runner: &dyn CommandRunner, out: &mut dyn Write) -> io::Result<bool> {
    let output = match runner.run("scrcpy", &["--version"]) {
        Ok(output) if output.success() => output,
        Ok(output) => {
            writeln!(out, "scrcpy 執行失敗：{}", output.stderr.trim())?;
            return Ok(false);
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            writeln!(out, "缺少 scrcpy，請安裝：{SCRCPY_INSTALL_HINT}")?;
            return Ok(false);
        }
        Err(err) => {
            writeln!(out, "無法執行 scrcpy：{err}")?;
            return Ok(false);
        }
    };
    match parse_scrcpy_version(&output.stdout) {
        Some(version) if version >= MIN_SCRCPY => {
            writeln!(out, "scrcpy：{version}")?;
            Ok(true)
        }
        Some(version) => {
            writeln!(
                out,
                "scrcpy {version} 不支援虛擬顯示（--new-display），需要 {MIN_SCRCPY} 以上，請更新：{SCRCPY_INSTALL_HINT}"
            )?;
            Ok(false)
        }
        None => {
            writeln!(out, "無法判斷 scrcpy 版本，需要 {MIN_SCRCPY} 以上")?;
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::runner::CommandOutput;

    const ADB_OUTPUT: &str = "Android Debug Bridge version 1.0.41\nVersion 35.0.2-android-tools\nInstalled as /usr/bin/adb\n";
    const SCRCPY_331_OUTPUT: &str = "scrcpy 3.3.1 <https://github.com/Genymobile/scrcpy>\n\nDependencies (compiled / linked):\n - SDL: 2.30.0 / 2.30.0\n";
    const SCRCPY_30_OUTPUT: &str = "scrcpy 3.0 <https://github.com/Genymobile/scrcpy>\n";
    const SCRCPY_225_OUTPUT: &str = "scrcpy 2.2.5 <https://github.com/Genymobile/scrcpy>\n";
    const ONE_DEVICE: &str = "List of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\n\n";
    const NO_DEVICE: &str = "List of devices attached\n\n";
    const TWO_DEVICES: &str = "List of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\nRFCX9876XYZ            device usb:3-4 product:gts9xxx model:SM_X710 device:gts9 transport_id:2\n\n";

    /// 假 runner 對某個工具的回應。
    #[derive(Clone, Copy)]
    enum Reply {
        /// 以 0 結束，stdout 為給定內容。
        Ok(&'static str),
        /// 以給定 code 結束，stderr 為給定內容。
        Exit(i32, &'static str),
        /// 程式不存在（`NotFound`）。
        Missing,
        /// 程式存在但無法啟動（`PermissionDenied`）。
        Denied,
    }

    /// 只回應 `adb version`、`scrcpy --version`、`adb devices -l`；參數不對就 panic，讓傳錯參數的實作測試失敗。
    struct FakeRunner {
        adb: Reply,
        scrcpy: Reply,
        devices: Reply,
        calls: RefCell<Vec<String>>,
    }

    impl FakeRunner {
        /// `adb devices -l` 預設回傳一台已授權的裝置。
        fn new(adb: Reply, scrcpy: Reply) -> Self {
            Self {
                adb,
                scrcpy,
                devices: Reply::Ok(ONE_DEVICE),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn with_devices(mut self, devices: Reply) -> Self {
            self.devices = devices;
            self
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            let reply = match (program, args) {
                ("adb", ["version"]) => self.adb,
                ("scrcpy", ["--version"]) => self.scrcpy,
                ("adb", ["devices", "-l"]) => self.devices,
                _ => panic!("unexpected command: {program} {args:?}"),
            };
            match reply {
                Reply::Ok(stdout) => Ok(CommandOutput {
                    code: Some(0),
                    stdout: stdout.to_string(),
                    stderr: String::new(),
                }),
                Reply::Exit(code, stderr) => Ok(CommandOutput {
                    code: Some(code),
                    stdout: String::new(),
                    stderr: stderr.to_string(),
                }),
                Reply::Missing => Err(io::Error::from(ErrorKind::NotFound)),
                Reply::Denied => Err(io::Error::from(ErrorKind::PermissionDenied)),
            }
        }
    }

    fn doctor(runner: &FakeRunner) -> (bool, String) {
        doctor_with_serial(runner, None)
    }

    fn doctor_with_serial(runner: &FakeRunner, serial: Option<&str>) -> (bool, String) {
        let mut out = Vec::new();
        let ok = run(runner, serial, &mut out).unwrap();
        (ok, String::from_utf8(out).unwrap())
    }

    #[test]
    fn parse_scrcpy_version_reads_three_part_version() {
        assert_eq!(
            parse_scrcpy_version("scrcpy 3.3.1 <https://github.com/Genymobile/scrcpy>"),
            Some(Version(3, 3, 1))
        );
    }

    #[test]
    fn parse_scrcpy_version_fills_missing_patch_with_zero() {
        assert_eq!(
            parse_scrcpy_version("scrcpy 3.0 <https://github.com/Genymobile/scrcpy>"),
            Some(Version(3, 0, 0))
        );
    }

    #[test]
    fn parse_scrcpy_version_rejects_other_program_output() {
        assert_eq!(parse_scrcpy_version("adb 3.3.1"), None);
    }

    #[test]
    fn parse_scrcpy_version_rejects_non_numeric_version() {
        assert_eq!(parse_scrcpy_version("scrcpy abc"), None);
    }

    #[test]
    fn parse_adb_version_reads_first_line() {
        assert_eq!(parse_adb_version(ADB_OUTPUT), Some("1.0.41"));
    }

    #[test]
    fn doctor_runs_adb_version_then_scrcpy_version_then_adb_devices() {
        let runner = FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Ok(SCRCPY_331_OUTPUT));

        doctor(&runner);

        assert_eq!(
            *runner.calls.borrow(),
            vec![
                "adb version".to_string(),
                "scrcpy --version".to_string(),
                "adb devices -l".to_string()
            ]
        );
    }

    #[test]
    fn doctor_without_adb_does_not_list_devices() {
        let runner = FakeRunner::new(Reply::Missing, Reply::Ok(SCRCPY_331_OUTPUT));

        doctor(&runner);

        assert_eq!(
            *runner.calls.borrow(),
            vec!["adb version".to_string(), "scrcpy --version".to_string()]
        );
    }

    #[test]
    fn doctor_with_tools_and_one_device_ready_succeeds_and_lists_them() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Ok(ADB_OUTPUT),
            Reply::Ok(SCRCPY_331_OUTPUT),
        ));

        assert!(ok);
        assert_eq!(
            out,
            "adb：1.0.41\nscrcpy：3.3.1\n裝置：R5CW1234ABC（SM_S918B）\n"
        );
    }

    #[test]
    fn doctor_without_device_fails_and_asks_for_data_cable() {
        let (ok, out) = doctor(
            &FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Ok(SCRCPY_331_OUTPUT))
                .with_devices(Reply::Ok(NO_DEVICE)),
        );

        assert!(!ok);
        assert!(out.ends_with(
            "找不到裝置：請用支援資料傳輸的 USB-C 線連接手機並開啟 USB 偵錯（有些 USB-C 線只能充電）\n"
        ));
    }

    #[test]
    fn doctor_with_two_devices_and_no_serial_fails_and_asks_for_serial() {
        let (ok, out) = doctor(
            &FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Ok(SCRCPY_331_OUTPUT))
                .with_devices(Reply::Ok(TWO_DEVICES)),
        );

        assert!(!ok);
        assert!(out.contains("偵測到多台裝置，請用 --serial 指定其中一台："));
    }

    #[test]
    fn doctor_with_serial_picks_that_device() {
        let (ok, out) = doctor_with_serial(
            &FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Ok(SCRCPY_331_OUTPUT))
                .with_devices(Reply::Ok(TWO_DEVICES)),
            Some("RFCX9876XYZ"),
        );

        assert!(ok);
        assert!(out.ends_with("裝置：RFCX9876XYZ（SM_X710）\n"));
    }

    #[test]
    fn doctor_with_unknown_serial_fails() {
        let (ok, out) = doctor_with_serial(
            &FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Ok(SCRCPY_331_OUTPUT)),
            Some("NOPE123"),
        );

        assert!(!ok);
        assert!(out.contains("找不到序號 NOPE123 的裝置"));
    }

    #[test]
    fn doctor_with_adb_devices_nonzero_exit_fails_and_reports_stderr() {
        let (ok, out) = doctor(
            &FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Ok(SCRCPY_331_OUTPUT))
                .with_devices(Reply::Exit(1, "error: protocol fault\n")),
        );

        assert!(!ok);
        assert!(out.ends_with("adb devices 執行失敗：error: protocol fault\n"));
    }

    #[test]
    fn doctor_with_scrcpy_exactly_3_0_succeeds() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Ok(ADB_OUTPUT),
            Reply::Ok(SCRCPY_30_OUTPUT),
        ));

        assert!(ok);
        assert_eq!(
            out,
            "adb：1.0.41\nscrcpy：3.0.0\n裝置：R5CW1234ABC（SM_S918B）\n"
        );
    }

    #[test]
    fn doctor_with_scrcpy_below_3_fails_and_reports_no_virtual_display() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Ok(ADB_OUTPUT),
            Reply::Ok(SCRCPY_225_OUTPUT),
        ));

        assert!(!ok);
        assert!(out.contains("scrcpy 2.2.5 不支援虛擬顯示（--new-display）"));
    }

    #[test]
    fn doctor_without_adb_fails_and_prints_dnf_install() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Missing,
            Reply::Ok(SCRCPY_331_OUTPUT),
        ));

        assert!(!ok);
        assert!(out.contains("缺少 adb，請安裝：sudo dnf install android-tools"));
    }

    #[test]
    fn doctor_without_scrcpy_fails_and_prints_copr_install() {
        let (ok, out) = doctor(&FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Missing));

        assert!(!ok);
        assert!(out.contains(
            "缺少 scrcpy，請安裝：sudo dnf copr enable zeno/scrcpy && sudo dnf install scrcpy"
        ));
    }

    #[test]
    fn doctor_without_both_tools_reports_both() {
        let (ok, out) = doctor(&FakeRunner::new(Reply::Missing, Reply::Missing));

        assert!(!ok);
        assert_eq!(
            out,
            "缺少 adb，請安裝：sudo dnf install android-tools\n缺少 scrcpy，請安裝：sudo dnf copr enable zeno/scrcpy && sudo dnf install scrcpy\n"
        );
    }

    #[test]
    fn doctor_with_adb_nonzero_exit_fails_reports_stderr_and_still_checks_scrcpy() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Exit(1, "adb: cannot connect to daemon\n"),
            Reply::Ok(SCRCPY_331_OUTPUT),
        ));

        assert!(!ok);
        assert_eq!(
            out,
            "adb 執行失敗：adb: cannot connect to daemon\nscrcpy：3.3.1\n"
        );
    }

    #[test]
    fn doctor_with_scrcpy_nonzero_exit_fails_reports_stderr_and_still_checks_adb() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Ok(ADB_OUTPUT),
            Reply::Exit(2, "ERROR: Could not load SDL\n"),
        ));

        assert!(!ok);
        assert_eq!(
            out,
            "adb：1.0.41\nscrcpy 執行失敗：ERROR: Could not load SDL\n裝置：R5CW1234ABC（SM_S918B）\n"
        );
    }

    #[test]
    fn doctor_with_adb_permission_denied_fails_reports_error_and_still_checks_scrcpy() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Denied,
            Reply::Ok(SCRCPY_331_OUTPUT),
        ));

        assert!(!ok);
        assert_eq!(out, "無法執行 adb：permission denied\nscrcpy：3.3.1\n");
    }

    #[test]
    fn doctor_with_scrcpy_permission_denied_fails_reports_error_and_still_checks_adb() {
        let (ok, out) = doctor(&FakeRunner::new(Reply::Ok(ADB_OUTPUT), Reply::Denied));

        assert!(!ok);
        assert_eq!(
            out,
            "adb：1.0.41\n無法執行 scrcpy：permission denied\n裝置：R5CW1234ABC（SM_S918B）\n"
        );
    }

    #[test]
    fn doctor_with_unparsable_scrcpy_version_fails() {
        let (ok, out) = doctor(&FakeRunner::new(
            Reply::Ok(ADB_OUTPUT),
            Reply::Ok("something else\n"),
        ));

        assert!(!ok);
        assert!(out.contains("無法判斷 scrcpy 版本"));
    }
}
