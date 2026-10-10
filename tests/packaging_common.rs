//! rpm 與 deb 共用的打包檔案：版號讀取腳本、系統用的 .desktop、dist/ 不進 git。

use std::path::Path;
use std::process::Command;

const GITIGNORE: &str = include_str!("../.gitignore");
const DESKTOP: &str = include_str!("../packaging/common/dex2pc.desktop");

fn version_sh(dir: &Path) -> String {
    run_version_sh(
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/packaging/common/version.sh"
        )),
        dir,
    )
}

fn run_version_sh(script: &Path, dir: &Path) -> String {
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
    assert_eq!(version_sh(Path::new(env!("CARGO_MANIFEST_DIR"))), "0.2.0\n");
}

#[test]
fn version_script_gives_same_result_from_another_working_directory() {
    assert_eq!(version_sh(Path::new("/")), "0.2.0\n");
}

/// 把 `cargo_toml` 放進暫存目錄裡的假專案，執行複製過去的 version.sh。
fn version_from_cargo_toml(cargo_toml: &str) -> std::process::Output {
    let root = tempfile::tempdir().unwrap();
    let common = root.path().join("packaging/common");
    std::fs::create_dir_all(&common).unwrap();
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/packaging/common/version.sh"),
        common.join("version.sh"),
    )
    .unwrap();
    std::fs::write(root.path().join("Cargo.toml"), cargo_toml).unwrap();
    Command::new("sh")
        .arg(common.join("version.sh"))
        .current_dir(root.path())
        .output()
        .unwrap()
}

fn stdout_of_successful_version_sh(cargo_toml: &str) -> String {
    let output = version_from_cargo_toml(cargo_toml);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn version_script_ignores_version_key_in_dependency_table() {
    assert_eq!(
        stdout_of_successful_version_sh(
            "[dependencies.foo]\nversion = \"9.9.9\"\n\n[package]\nname = \"x\"\nversion = \"1.2.3\"\n"
        ),
        "1.2.3\n"
    );
}

#[test]
fn version_script_reads_package_section_header_followed_by_comment() {
    assert_eq!(
        stdout_of_successful_version_sh(
            "[package] # 套件資訊\nname = \"x\"\nversion = \"1.2.3\"\n"
        ),
        "1.2.3\n"
    );
}

#[test]
fn version_script_reads_indented_version_line() {
    assert_eq!(
        stdout_of_successful_version_sh("[package]\nname = \"x\"\n    version   =   \"1.2.3\"\n"),
        "1.2.3\n"
    );
}

#[test]
fn version_script_reads_single_quoted_version() {
    assert_eq!(
        stdout_of_successful_version_sh("[package]\nversion = '1.2.3'\n"),
        "1.2.3\n"
    );
}

#[test]
fn version_script_drops_comment_after_version_value() {
    assert_eq!(
        stdout_of_successful_version_sh("[package]\nversion = \"1.2.3\" # 進版時改這裡\n"),
        "1.2.3\n"
    );
}

#[test]
fn version_script_fails_when_package_has_no_version() {
    let output = version_from_cargo_toml(
        "[package]\nname = \"x\"\n\n[dependencies.foo]\nversion = \"9.9.9\"\n",
    );

    assert!(!output.status.success());
    assert_eq!(output.stdout, b"");
}

#[test]
fn gitignore_ignores_files_inside_dist_directory() {
    let status = Command::new("git")
        .args(["check-ignore", "-q", "dist/dex2pc-0.2.0.rpm"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .unwrap();
    assert!(status.success(), "dist/ 內的檔案應被 .gitignore 排除");
}

#[test]
fn git_status_stays_clean_after_dist_output_is_created() {
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        output.stdout
    };
    git(&["init", "-q"]);
    std::fs::write(repo.path().join(".gitignore"), GITIGNORE).unwrap();
    git(&["add", ".gitignore"]);
    git(&[
        "-c",
        "user.name=test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-q",
        "-m",
        "init",
    ]);
    std::fs::create_dir(repo.path().join("dist")).unwrap();
    std::fs::write(repo.path().join("dist/dex2pc-0.2.0-1.x86_64.rpm"), b"").unwrap();

    assert_eq!(git(&["status", "--porcelain"]), b"");
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

#[test]
fn version_script_reads_cargo_toml_with_crlf_line_endings() {
    assert_eq!(
        stdout_of_successful_version_sh(
            "[package]\r\nname = \"x\"\r\nversion = \"1.2.3\"\r\n\r\n[dependencies]\r\n"
        ),
        "1.2.3\n"
    );
}

#[test]
fn version_script_reads_crlf_package_header_followed_by_comment() {
    assert_eq!(
        stdout_of_successful_version_sh("[package] # c\r\nversion = '1.2.3' # d\r\n"),
        "1.2.3\n"
    );
}
