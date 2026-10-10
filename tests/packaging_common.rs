//! rpm 與 deb 共用的打包檔案：版號讀取腳本、系統用的 .desktop、dist/ 不進 git。

use std::path::Path;
use std::process::Command;

const GITIGNORE: &str = include_str!("../.gitignore");
const DESKTOP: &str = include_str!("../packaging/common/dex2pc.desktop");

fn version_sh(dir: &Path) -> String {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/packaging/common/version.sh");
    let output = Command::new("sh")
        .arg(script)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn version_script_prints_cargo_toml_version_with_single_newline() {
    assert_eq!(
        version_sh(Path::new(env!("CARGO_MANIFEST_DIR"))),
        concat!(env!("CARGO_PKG_VERSION"), "\n")
    );
}

#[test]
fn version_script_gives_same_result_from_another_working_directory() {
    assert_eq!(
        version_sh(Path::new("/")),
        concat!(env!("CARGO_PKG_VERSION"), "\n")
    );
}

#[test]
fn desktop_file_execs_plain_dex2pc_start_without_watch() {
    assert!(DESKTOP.lines().any(|line| line == "Exec=dex2pc start"));
}

#[test]
fn desktop_file_passes_desktop_file_validate() {
    let status = Command::new("desktop-file-validate")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/packaging/common/dex2pc.desktop"
        ))
        .status();
    match status {
        Ok(status) => assert!(status.success()),
        // 本機沒裝 desktop-file-utils 時略過；CI 一定要裝
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            assert!(
                std::env::var_os("CI").is_none(),
                "CI 需要安裝 desktop-file-utils"
            );
            eprintln!("desktop-file-validate 不存在，略過");
        }
        Err(err) => panic!("{err}"),
    }
}

#[test]
fn gitignore_excludes_dist_directory() {
    assert!(GITIGNORE.lines().any(|line| line == "dist/"));
}
