//! rpm 打包：spec 能解析、版號和 Cargo.toml 一致、build-rpm.sh 真的產出 rpm。
//! 需要 rpmbuild / rpmspec；本機沒裝時略過，CI（設了 `CI` 環境變數）沒裝就失敗。

use std::path::Path;
use std::process::{Command, Output};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// 在專案根目錄執行 `program`；程式不存在時回傳 `None`（本機略過，CI 失敗）。
fn run_in_root(program: &str, args: &[&str]) -> Option<Output> {
    run_in_dir(program, args, Path::new(ROOT))
}

/// 在 `dir` 執行 `program`；程式不存在時回傳 `None`（本機略過，CI 失敗）。
fn run_in_dir(program: &str, args: &[&str], dir: &Path) -> Option<Output> {
    match Command::new(program).args(args).current_dir(dir).output() {
        Ok(output) => Some(output),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI 需要安裝 {program}（rpm-build）"
            );
            eprintln!("{program} 不存在，略過");
            None
        }
        Err(err) => panic!("{err}"),
    }
}

#[test]
fn spec_parses_without_errors_or_warnings() {
    let Some(output) = run_in_root("rpmspec", &["-P", "packaging/rpm/dex2pc.spec"]) else {
        return;
    };

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn spec_version_equals_cargo_toml_version() {
    let Some(output) = run_in_root(
        "rpmspec",
        &["-q", "--qf", "%{version}\\n", "packaging/rpm/dex2pc.spec"],
    ) else {
        return;
    };

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0.2.0\n");
}

#[test]
fn spec_version_is_read_from_cargo_toml_of_the_project() {
    // 把 spec 和 version.sh 放進另一個假專案，版號不會是 0.2.0；寫死在 spec 裡就會失敗
    let project = tempfile::tempdir().unwrap();
    let common = project.path().join("packaging/common");
    let rpm_dir = project.path().join("packaging/rpm");
    std::fs::create_dir_all(&common).unwrap();
    std::fs::create_dir_all(&rpm_dir).unwrap();
    std::fs::copy(
        Path::new(ROOT).join("packaging/common/version.sh"),
        common.join("version.sh"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(ROOT).join("packaging/rpm/dex2pc.spec"),
        rpm_dir.join("dex2pc.spec"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("Cargo.toml"),
        "[package]\nname = \"dex2pc\"\nversion = \"7.8.9\"\n",
    )
    .unwrap();

    let Some(output) = run_in_dir(
        "rpmspec",
        &["-q", "--qf", "%{version}\\n", "packaging/rpm/dex2pc.spec"],
        project.path(),
    ) else {
        return;
    };

    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "7.8.9\n");
}

#[test]
fn spec_targets_only_x86_64() {
    let Some(output) = run_in_root("rpmspec", &["-P", "packaging/rpm/dex2pc.spec"]) else {
        return;
    };

    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .any(|line| line.split_whitespace().eq(["ExclusiveArch:", "x86_64"]))
    );
}

#[test]
fn build_script_produces_rpm_named_after_cargo_toml_version() {
    if run_in_root("rpmbuild", &["--version"]).is_none() {
        return;
    }
    let Some(output) = run_in_root("sh", &["packaging/build-rpm.sh"]) else {
        return;
    };

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // 檔名中間的 dist 標籤（.fc44 等）依建置的主機而定，只檢查前後固定的部分
    let rpm = stdout.lines().last().unwrap();
    assert!(rpm.starts_with("dist/dex2pc-0.2.0-1."), "{rpm}");
    assert!(rpm.ends_with(".x86_64.rpm"), "{rpm}");
    assert!(Path::new(ROOT).join(rpm).is_file());

    // 產物是 x86_64 的 binary 套件（不是 src.rpm）
    let Some(query) = run_in_root("rpm", &["-qp", "--qf", "%{ARCH}\\n", rpm]) else {
        return;
    };
    assert!(query.status.success(), "{query:?}");
    assert_eq!(String::from_utf8(query.stdout).unwrap(), "x86_64\n");
}

#[test]
fn build_script_lists_only_the_rpm_it_just_built() {
    if run_in_root("rpmbuild", &["--version"]).is_none() {
        return;
    }
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(dist.path().join("dex2pc-0.1.0-1.fc44.x86_64.rpm"), b"").unwrap();

    let output = Command::new("sh")
        .arg("packaging/build-rpm.sh")
        .current_dir(ROOT)
        .env("DEX2PC_DIST_DIR", dist.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // rpmbuild 也會印 "Wrote: .../RPMS/...rpm"，所以只數落在 dist 底下的路徑
    let listed: Vec<&str> = stdout
        .lines()
        .filter(|line| Path::new(line).starts_with(dist.path()))
        .collect();
    assert_eq!(listed.len(), 1, "{stdout}");
    assert!(listed[0].contains("/dex2pc-0.2.0-1."), "{stdout}");
}

#[test]
fn build_script_succeeds_when_cargo_target_dir_points_elsewhere() {
    if run_in_root("rpmbuild", &["--version"]).is_none() {
        return;
    }
    let target = tempfile::tempdir().unwrap();
    let dist = tempfile::tempdir().unwrap();

    let output = Command::new("sh")
        .arg("packaging/build-rpm.sh")
        .current_dir(ROOT)
        .env("CARGO_TARGET_DIR", target.path())
        .env("DEX2PC_DIST_DIR", dist.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    // rpmbuild 自己的輸出也在 stdout，腳本印的 rpm 路徑是最後一行
    let rpm = stdout.lines().last().unwrap();
    assert!(Path::new(rpm).starts_with(dist.path()), "{rpm}");
    assert!(Path::new(rpm).is_file(), "{rpm}");
}
