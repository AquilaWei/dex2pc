//! deb 打包：build-deb.sh 真的產出 .deb，欄位、相依與檔案都正確。
//! 需要 dpkg-deb 等工具；本機沒裝時略過，CI（設了 `CI` 環境變數）沒裝就失敗。
//! 整組測試共用同一次建置。

mod common;
use common::require_tools;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// 本次測試程式專屬的 build 產物；暫存目錄放在 static 裡所以活到測試程式結束
/// （static 不會 drop，目錄留在系統暫存區由 OS 清理）。
struct Built {
    _dist: tempfile::TempDir,
    deb: PathBuf,
}

fn built_deb() -> Option<&'static Path> {
    static BUILT: OnceLock<Option<Built>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            // build-deb.sh 用到 cargo、git、gzip、strip（install -s）、dpkg-deb；測試另外用 desktop-file-validate 以外的工具
            if !require_tools(&["dpkg-deb", "cargo", "git", "gzip", "strip", "du"]) {
                return None;
            }
            let dist = tempfile::tempdir().unwrap();
            let output = Command::new("sh")
                .arg("packaging/build-deb.sh")
                .current_dir(ROOT)
                .env("DEX2PC_DIST_DIR", dist.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            let stdout = String::from_utf8(output.stdout).unwrap();
            let deb = PathBuf::from(stdout.lines().last().unwrap());
            Some(Built { _dist: dist, deb })
        })
        .as_ref()
        .map(|built| built.deb.as_path())
}

fn dpkg_deb(args: &[&str]) -> Option<String> {
    let deb = built_deb()?;
    let output: Output = Command::new("dpkg-deb")
        .args(args)
        .arg(deb)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    Some(String::from_utf8(output.stdout).unwrap())
}

fn field(name: &str) -> Option<String> {
    let deb = built_deb()?;
    let output = Command::new("dpkg-deb")
        .args(["-f"])
        .arg(deb)
        .arg(name)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    Some(String::from_utf8(output.stdout).unwrap())
}

#[test]
fn deb_version_equals_cargo_toml_version_without_revision() {
    let Some(version) = field("Version") else {
        return;
    };

    assert_eq!(version, "0.2.0\n");
}

#[test]
fn deb_is_named_after_version_and_amd64() {
    let Some(deb) = built_deb() else {
        return;
    };

    assert_eq!(
        deb.file_name().unwrap().to_str().unwrap(),
        "dex2pc_0.2.0_amd64.deb"
    );
}

#[test]
fn deb_architecture_is_amd64() {
    let Some(arch) = field("Architecture") else {
        return;
    };

    assert_eq!(arch, "amd64\n");
}

#[test]
fn deb_depends_on_adb_and_scrcpy_3_2_as_hard_requirements() {
    let Some(depends) = field("Depends") else {
        return;
    };

    let depends: Vec<&str> = depends.trim_end().split(", ").collect();
    assert!(depends.contains(&"android-tools-adb"), "{depends:?}");
    assert!(depends.contains(&"scrcpy (>= 3.2)"), "{depends:?}");
}

#[test]
fn deb_does_not_merely_recommend_scrcpy() {
    let Some(recommends) = field("Recommends") else {
        return;
    };

    // dpkg-deb -f 對不存在的欄位只印一個空行
    assert_eq!(recommends, "\n");
}

#[test]
fn deb_installs_binary_desktop_file_readme_copyright_and_changelog() {
    let Some(listing) = dpkg_deb(&["-c"]) else {
        return;
    };

    for expected in [
        "./usr/bin/dex2pc",
        "./usr/share/applications/dex2pc.desktop",
        "./usr/share/doc/dex2pc/README.md",
        "./usr/share/doc/dex2pc/copyright",
        "./usr/share/doc/dex2pc/changelog.Debian.gz",
    ] {
        assert!(
            listing.lines().any(|line| line.ends_with(expected)),
            "{expected} 不在\n{listing}"
        );
    }
}

#[test]
fn deb_contains_no_build_output_and_files_are_owned_by_root() {
    let Some(listing) = dpkg_deb(&["-c"]) else {
        return;
    };

    assert!(!listing.contains("target"), "{listing}");
    assert!(
        listing
            .lines()
            .all(|line| line.split_whitespace().nth(1) == Some("root/root")),
        "{listing}"
    );
}

#[test]
fn packaged_desktop_file_equals_shared_packaging_copy() {
    let Some(deb) = built_deb() else {
        return;
    };
    let extract = tempfile::tempdir().unwrap();

    let status = Command::new("dpkg-deb")
        .arg("-x")
        .arg(deb)
        .arg(extract.path())
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(
        std::fs::read(extract.path().join("usr/share/applications/dex2pc.desktop")).unwrap(),
        std::fs::read(Path::new(ROOT).join("packaging/common/dex2pc.desktop")).unwrap()
    );
}

#[test]
fn packaged_desktop_file_passes_desktop_file_validate() {
    let Some(deb) = built_deb() else {
        return;
    };
    if !require_tools(&["desktop-file-validate"]) {
        return;
    }
    let extract = tempfile::tempdir().unwrap();
    let status = Command::new("dpkg-deb")
        .arg("-x")
        .arg(deb)
        .arg(extract.path())
        .status()
        .unwrap();
    assert!(status.success());

    let validate = Command::new("desktop-file-validate")
        .arg(extract.path().join("usr/share/applications/dex2pc.desktop"))
        .output()
        .unwrap();

    assert!(validate.status.success(), "{validate:?}");
}

#[test]
fn scrcpy_minimum_matches_doctor_and_build_script() {
    // 三處要一起改：src/doctor.rs、rpm spec、build-deb.sh；一邊改了另一邊要跟著改
    let doctor = std::fs::read_to_string(Path::new(ROOT).join("src/doctor.rs")).unwrap();
    let script = std::fs::read_to_string(Path::new(ROOT).join("packaging/build-deb.sh")).unwrap();

    assert!(doctor.contains("pub const MIN_SCRCPY: Version = Version(3, 2, 0);"));
    assert!(script.lines().any(|line| line == "min_scrcpy=3.2"));
}

#[test]
fn packaging_readme_documents_deb_dependency_names_and_build_tool_choice() {
    let readme = std::fs::read_to_string(Path::new(ROOT).join("packaging/README.md")).unwrap();

    assert!(readme.contains("android-tools-adb"));
    assert!(readme.contains("scrcpy (>= 3.2)"));
    assert!(readme.contains("cargo-deb"));
    assert!(readme.contains("dpkg-deb"));
    assert!(readme.contains("packaging/common") || readme.contains("`common/"));
}

#[test]
fn deb_depends_cover_every_shared_library_the_binary_needs() {
    let Some(deb) = built_deb() else {
        return;
    };
    if !require_tools(&["readelf"]) {
        return;
    }
    let extract = tempfile::tempdir().unwrap();
    let status = Command::new("dpkg-deb")
        .arg("-x")
        .arg(deb)
        .arg(extract.path())
        .status()
        .unwrap();
    assert!(status.success());

    // 二進位直接連結的函式庫；ld-linux 由 libc6 提供。新增連結的函式庫時要同時補 Depends
    let output = Command::new("readelf")
        .args(["-d"])
        .arg(extract.path().join("usr/bin/dex2pc"))
        .output()
        .unwrap();
    let readelf = String::from_utf8(output.stdout).unwrap();
    let needed: Vec<&str> = readelf
        .lines()
        .filter(|line| line.contains("(NEEDED)"))
        .map(|line| line.split(['[', ']']).nth(1).unwrap())
        .collect();
    assert_eq!(
        needed,
        ["libgcc_s.so.1", "libc.so.6", "ld-linux-x86-64.so.2"]
    );

    let depends = field("Depends").unwrap();
    assert!(depends.contains("libc6"), "{depends}");
    assert!(depends.contains("libgcc-s1"), "{depends}");
}

#[test]
fn cargo_toml_does_not_pull_in_cargo_deb() {
    // 決定用 dpkg-deb 不用 cargo-deb：不新增建置用的 crate，也不在 Cargo.toml 寫 deb 的 metadata
    let cargo_toml = std::fs::read_to_string(Path::new(ROOT).join("Cargo.toml")).unwrap();

    assert!(!cargo_toml.contains("cargo-deb"), "{cargo_toml}");
    assert!(!cargo_toml.contains("metadata.deb"), "{cargo_toml}");
}
