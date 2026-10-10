//! 建出來的 rpm 裝了什麼、相依什麼、rpmlint 有沒有意見。
//! 整組測試共用同一次 build-rpm.sh（約 20 秒）；缺工具時本機略過，設了 `CI` 就失敗。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

mod common;
use common::require_tools;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn run(program: &str, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(ROOT)
        .output()
        .unwrap()
}

/// 本次測試程式專屬的 build 產物。放在 static 裡，所以暫存目錄活到測試程式結束
/// （static 不會 drop，目錄留在系統暫存區由 OS 清；每個測試程式各有自己的目錄，互不干擾）。
struct Built {
    _dist: tempfile::TempDir,
    rpm: PathBuf,
}

/// 建一次 rpm 並在整個測試程式內共用；工具缺少時回傳 `None`。
fn built_rpm() -> Option<&'static Path> {
    static BUILT: OnceLock<Option<Built>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            // build-rpm.sh 用到 git、tar、cargo、rpmbuild，spec 的 %check 用到 desktop-file-validate，
            // 這個檔案的其他測試還會用 rpm、rpm2cpio、cpio
            if !require_tools(&[
                "rpmbuild",
                "desktop-file-validate",
                "git",
                "tar",
                "cargo",
                "rpm",
                "rpm2cpio",
                "cpio",
            ]) {
                return None;
            }
            let dist = tempfile::tempdir().unwrap();
            let output = Command::new("sh")
                .arg("packaging/build-rpm.sh")
                .current_dir(ROOT)
                .env("DEX2PC_DIST_DIR", dist.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            let rpm = PathBuf::from(stdout.lines().last().unwrap());
            Some(Built { _dist: dist, rpm })
        })
        .as_ref()
        .map(|built| built.rpm.as_path())
}

fn rpm_query(args: &[&str]) -> Option<String> {
    let rpm = built_rpm()?;
    let mut full: Vec<&str> = vec!["-qp"];
    full.extend_from_slice(args);
    full.push(rpm.to_str().unwrap());
    let output = run("rpm", &full);
    assert!(output.status.success(), "{output:?}");
    Some(String::from_utf8(output.stdout).unwrap())
}

#[test]
fn rpm_installs_binary_license_readme_and_system_desktop_file() {
    let Some(files) = rpm_query(&["-l"]) else {
        return;
    };

    let files: Vec<&str> = files.lines().collect();
    for expected in [
        "/usr/bin/dex2pc",
        "/usr/share/applications/dex2pc.desktop",
        "/usr/share/licenses/dex2pc/LICENSE",
        "/usr/share/doc/dex2pc/README.md",
    ] {
        assert!(files.contains(&expected), "{expected} 不在 {files:?}");
    }
}

#[test]
fn rpm_contains_no_build_output_directories() {
    let Some(files) = rpm_query(&["-l"]) else {
        return;
    };

    assert!(
        !files.lines().any(|line| line.contains("target")),
        "{files}"
    );
}

#[test]
fn rpm_requires_android_tools_and_scrcpy_3_2() {
    let Some(requires) = rpm_query(&["-R"]) else {
        return;
    };

    let requires: Vec<&str> = requires.lines().collect();
    assert!(requires.contains(&"android-tools"), "{requires:?}");
    assert!(requires.contains(&"scrcpy >= 3.2"), "{requires:?}");
}

#[test]
fn rpm_does_not_merely_recommend_scrcpy() {
    let Some(recommends) = rpm_query(&["--recommends"]) else {
        return;
    };

    assert_eq!(recommends, "");
}

/// 把 `rpm` 解到 `dest`。`rpm2cpio` 與 `cpio` 兩個行程都要成功，只看 cpio 的話
/// 壞掉的 rpm（rpm2cpio 失敗、cpio 讀到空輸入）會被當成成功。
fn extract_rpm(converter: &Path, rpm: &Path, dest: &Path) -> Result<(), String> {
    let mut rpm2cpio = Command::new(converter)
        .arg(rpm)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("rpm2cpio 啟動失敗：{err}"))?;
    let cpio = Command::new("cpio")
        .args(["-id", "--quiet"])
        .current_dir(dest)
        .stdin(rpm2cpio.stdout.take().unwrap())
        .status()
        .map_err(|err| format!("cpio 啟動失敗：{err}"))?;
    let rpm2cpio = rpm2cpio
        .wait()
        .map_err(|err| format!("等待 rpm2cpio 失敗：{err}"))?;
    if !rpm2cpio.success() {
        return Err(format!("rpm2cpio 失敗：{rpm2cpio}"));
    }
    if !cpio.success() {
        return Err(format!("cpio 失敗：{cpio}"));
    }
    Ok(())
}

/// 把建好的 rpm 解到暫存目錄，回傳目錄；rpm 不存在時回傳 `None`。
fn extract_built_rpm() -> Option<tempfile::TempDir> {
    let rpm = built_rpm()?;
    let extract = tempfile::tempdir().unwrap();
    extract_rpm(Path::new("rpm2cpio"), rpm, extract.path()).unwrap();
    Some(extract)
}

#[test]
fn extract_rpm_fails_when_rpm2cpio_fails_even_if_cpio_succeeds() {
    if !require_tools(&["cpio"]) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // 假的 rpm2cpio：輸出一個合法但空的 cpio 歸檔（cpio 會成功），然後以失敗結束
    let fake = dir.path().join("fake-rpm2cpio");
    std::fs::write(
        &fake,
        "#!/bin/sh\ncpio -o --format=newc </dev/null\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let result = extract_rpm(&fake, Path::new("unused.rpm"), dir.path());

    assert!(result.is_err(), "{result:?}");
}

#[test]
fn packaged_desktop_file_runs_dex2pc_start_and_validates() {
    let Some(extract) = extract_built_rpm() else {
        return;
    };
    let desktop = extract.path().join("usr/share/applications/dex2pc.desktop");

    let content = std::fs::read_to_string(&desktop).unwrap();
    assert!(content.lines().any(|line| line == "Exec=dex2pc start"));
    let validate = run("desktop-file-validate", &[desktop.to_str().unwrap()]);
    assert!(validate.status.success(), "{validate:?}");
}

#[test]
fn packaged_desktop_file_equals_shared_packaging_copy() {
    let Some(extract) = extract_built_rpm() else {
        return;
    };

    let packaged = extract.path().join("usr/share/applications/dex2pc.desktop");
    assert_eq!(
        std::fs::read(packaged).unwrap(),
        std::fs::read(Path::new(ROOT).join("packaging/common/dex2pc.desktop")).unwrap()
    );
}

#[test]
fn packaged_license_and_readme_equal_repo_files() {
    let Some(extract) = extract_built_rpm() else {
        return;
    };

    assert_eq!(
        std::fs::read(extract.path().join("usr/share/licenses/dex2pc/LICENSE")).unwrap(),
        std::fs::read(Path::new(ROOT).join("LICENSE")).unwrap()
    );
    assert_eq!(
        std::fs::read(extract.path().join("usr/share/doc/dex2pc/README.md")).unwrap(),
        std::fs::read(Path::new(ROOT).join("README.md")).unwrap()
    );
}

#[test]
fn spec_scrcpy_requirement_matches_min_scrcpy_in_doctor() {
    // doctor.rs 是程式內檢查的版本；spec 的 Requires 要跟它一樣，一邊改另一邊要跟著改
    let doctor = std::fs::read_to_string(Path::new(ROOT).join("src/doctor.rs")).unwrap();
    let spec = std::fs::read_to_string(Path::new(ROOT).join("packaging/rpm/dex2pc.spec")).unwrap();

    assert!(doctor.contains("pub const MIN_SCRCPY: Version = Version(3, 2, 0);"));
    assert!(spec.lines().any(|line| line.split_whitespace().eq([
        "Requires:",
        "scrcpy",
        ">=",
        "3.2"
    ])));
}

#[test]
fn rpmlint_reports_no_errors_with_project_filters() {
    let Some(rpm) = built_rpm() else {
        return;
    };

    if !require_tools(&["rpmlint"]) {
        return;
    }
    let output = run(
        "rpmlint",
        &[
            "-r",
            "packaging/rpm/dex2pc.rpmlintrc",
            rpm.to_str().unwrap(),
        ],
    );

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("0 errors, 0 warnings"), "{stdout}");
}
