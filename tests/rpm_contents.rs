//! 建出來的 rpm 裝了什麼、相依什麼、rpmlint 有沒有意見。
//! 整組測試共用同一次 build-rpm.sh（約 20 秒）；缺工具時本機略過，設了 `CI` 就失敗。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn run(program: &str, args: &[&str]) -> Option<Output> {
    match Command::new(program).args(args).current_dir(ROOT).output() {
        Ok(output) => Some(output),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            assert!(std::env::var_os("CI").is_none(), "CI 需要安裝 {program}");
            eprintln!("{program} 不存在，略過");
            None
        }
        Err(err) => panic!("{err}"),
    }
}

/// 建一次 rpm 並在整個測試程式內共用；工具缺少時回傳 `None`。
fn built_rpm() -> Option<&'static Path> {
    static RPM: OnceLock<Option<PathBuf>> = OnceLock::new();
    RPM.get_or_init(|| {
        run("rpmbuild", &["--version"])?;
        // 放在 target/ 底下（已被 git 忽略），不碰使用者的 dist/
        let dist = Path::new(ROOT).join("target/rpm-contents-dist");
        let _ = std::fs::remove_dir_all(&dist);
        let output = Command::new("sh")
            .arg("packaging/build-rpm.sh")
            .current_dir(ROOT)
            .env("DEX2PC_DIST_DIR", &dist)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        Some(PathBuf::from(stdout.lines().last().unwrap()))
    })
    .as_deref()
}

fn rpm_query(args: &[&str]) -> Option<String> {
    let rpm = built_rpm()?;
    let mut full: Vec<&str> = vec!["-qp"];
    full.extend_from_slice(args);
    full.push(rpm.to_str().unwrap());
    let output = run("rpm", &full)?;
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

/// 把建好的 rpm 解到暫存目錄，回傳目錄；rpm 不存在時回傳 `None`。
fn extract_built_rpm() -> Option<tempfile::TempDir> {
    let rpm = built_rpm()?;
    let extract = tempfile::tempdir().unwrap();
    let status = Command::new("sh")
        .args(["-c", r#"rpm2cpio "$1" | cpio -id --quiet"#, "sh"])
        .arg(rpm)
        .current_dir(extract.path())
        .status()
        .unwrap();
    assert!(status.success());
    Some(extract)
}

#[test]
fn packaged_desktop_file_runs_dex2pc_start_and_validates() {
    let Some(extract) = extract_built_rpm() else {
        return;
    };
    let desktop = extract.path().join("usr/share/applications/dex2pc.desktop");

    let content = std::fs::read_to_string(&desktop).unwrap();
    assert!(content.lines().any(|line| line == "Exec=dex2pc start"));
    let Some(validate) = run("desktop-file-validate", &[desktop.to_str().unwrap()]) else {
        return;
    };
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

    let Some(output) = run(
        "rpmlint",
        &[
            "-r",
            "packaging/rpm/dex2pc.rpmlintrc",
            rpm.to_str().unwrap(),
        ],
    ) else {
        return;
    };

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("0 errors, 0 warnings"), "{stdout}");
}
