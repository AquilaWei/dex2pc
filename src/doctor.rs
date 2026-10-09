use std::fmt;
use std::io::{self, ErrorKind, Write};

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

/// 檢查電腦端的 adb 與 scrcpy，把結果寫到 `out`。
///
/// 兩項都會檢查完才結束，讓使用者一次看到所有問題。回傳 `Ok(true)` 表示全部就緒。
///
/// # Errors
/// 只有寫入 `out` 失敗時回傳錯誤；工具缺少或版本不符是回傳 `Ok(false)`。
pub fn run(runner: &dyn CommandRunner, out: &mut dyn Write) -> io::Result<bool> {
    let adb_ok = check_adb(runner, out)?;
    let scrcpy_ok = check_scrcpy(runner, out)?;
    Ok(adb_ok && scrcpy_ok)
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
    use super::*;
    use crate::runner::CommandOutput;

    const ADB_OUTPUT: &str = "Android Debug Bridge version 1.0.41\nVersion 35.0.2-android-tools\nInstalled as /usr/bin/adb\n";
    const SCRCPY_331_OUTPUT: &str = "scrcpy 3.3.1 <https://github.com/Genymobile/scrcpy>\n\nDependencies (compiled / linked):\n - SDL: 2.30.0 / 2.30.0\n";
    const SCRCPY_225_OUTPUT: &str = "scrcpy 2.2.5 <https://github.com/Genymobile/scrcpy>\n";

    /// `None` 代表該程式不存在（回傳 `NotFound`）。
    struct FakeRunner {
        adb: Option<&'static str>,
        scrcpy: Option<&'static str>,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, _args: &[&str]) -> io::Result<CommandOutput> {
            let stdout = match program {
                "adb" => self.adb,
                "scrcpy" => self.scrcpy,
                _ => None,
            };
            stdout
                .map(|s| CommandOutput {
                    code: Some(0),
                    stdout: s.to_string(),
                    stderr: String::new(),
                })
                .ok_or_else(|| io::Error::from(ErrorKind::NotFound))
        }
    }

    fn doctor(runner: &FakeRunner) -> (bool, String) {
        let mut out = Vec::new();
        let ok = run(runner, &mut out).unwrap();
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
    fn doctor_with_both_tools_ready_succeeds_and_lists_versions() {
        let (ok, out) = doctor(&FakeRunner {
            adb: Some(ADB_OUTPUT),
            scrcpy: Some(SCRCPY_331_OUTPUT),
        });

        assert!(ok);
        assert_eq!(out, "adb：1.0.41\nscrcpy：3.3.1\n");
    }

    #[test]
    fn doctor_with_scrcpy_below_3_fails_and_reports_no_virtual_display() {
        let (ok, out) = doctor(&FakeRunner {
            adb: Some(ADB_OUTPUT),
            scrcpy: Some(SCRCPY_225_OUTPUT),
        });

        assert!(!ok);
        assert!(out.contains("scrcpy 2.2.5 不支援虛擬顯示（--new-display）"));
    }

    #[test]
    fn doctor_without_adb_fails_and_prints_dnf_install() {
        let (ok, out) = doctor(&FakeRunner {
            adb: None,
            scrcpy: Some(SCRCPY_331_OUTPUT),
        });

        assert!(!ok);
        assert!(out.contains("缺少 adb，請安裝：sudo dnf install android-tools"));
    }

    #[test]
    fn doctor_without_scrcpy_fails_and_prints_copr_install() {
        let (ok, out) = doctor(&FakeRunner {
            adb: Some(ADB_OUTPUT),
            scrcpy: None,
        });

        assert!(!ok);
        assert!(out.contains(
            "缺少 scrcpy，請安裝：sudo dnf copr enable zeno/scrcpy && sudo dnf install scrcpy"
        ));
    }

    #[test]
    fn doctor_without_both_tools_reports_both() {
        let (ok, out) = doctor(&FakeRunner {
            adb: None,
            scrcpy: None,
        });

        assert!(!ok);
        assert_eq!(
            out,
            "缺少 adb，請安裝：sudo dnf install android-tools\n缺少 scrcpy，請安裝：sudo dnf copr enable zeno/scrcpy && sudo dnf install scrcpy\n"
        );
    }

    #[test]
    fn doctor_with_unparsable_scrcpy_version_fails() {
        let (ok, out) = doctor(&FakeRunner {
            adb: Some(ADB_OUTPUT),
            scrcpy: Some("something else\n"),
        });

        assert!(!ok);
        assert!(out.contains("無法判斷 scrcpy 版本"));
    }
}
