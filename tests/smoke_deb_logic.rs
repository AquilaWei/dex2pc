//! packaging/smoke-test-deb-inner.sh 與 smoke-test-deb.sh 的判斷邏輯。
//! 用假的 apt-get / apt-cache / dpkg / equivs-build / dex2pc / podman 取代真實指令，
//! 不需要網路、容器或 deb，所以本機與 CI 都會跑。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// 同一支假指令腳本，依被呼叫的名字決定行為，行為由 FAKE_* 環境變數控制；
/// 每次呼叫都記到 `$FAKE_LOG`，測試用它確認「有沒有呼叫 `equivs-build`」之類的事。
const FAKE: &str = r#"#!/bin/sh
name=$(basename "$0")
echo "$name $*" >>"${FAKE_LOG:-/dev/null}"
case "$name" in
apt-get)
    step=other
    case "$*" in
    *update*) step=update ;;
    *remove*) step=remove ;;
    *equivs*) step=equivs ;;
    *scrcpy_*) step=fake-scrcpy ;;
    *.deb) step=install ;;
    esac
    if [ "${FAKE_FAIL_STEP:-}" = "$step" ]; then
        printf '%s\n' "$FAKE_FAIL_MESSAGE" >&2
        exit 100
    fi
    ;;
apt-cache)
    if [ -n "${FAKE_APT_CACHE_FAIL:-}" ]; then
        echo "E: The package cache file is corrupted" >&2
        exit 100
    fi
    case "$2" in
    scrcpy)
        candidate=${FAKE_SCRCPY_CANDIDATE-3.3.4-1}
        # 倉庫沒有這個套件時，真的 apt-cache policy 不印東西但退出 0
        if [ -n "$candidate" ]; then
            printf 'scrcpy:\n  Installed: (none)\n  Candidate: %s\n' "$candidate"
        fi
        exit 0
        ;;
    adb) printf 'adb:\n  Installed: (none)\n  Candidate: 1:34.0.5-12\n' ;;
    esac
    ;;
dpkg)
    # dpkg --compare-versions A (ge|gt) B
    [ "$(printf '%s\n%s\n' "$2" "$4" | sort -V | tail -1)" = "$2" ] || exit 1
    [ "$3" = ge ] || [ "$2" != "$4" ]
    ;;
equivs-build)
    cp "$1" "$FAKE_CONTROL_COPY"
    : >scrcpy_3.2_all.deb
    ;;
dex2pc)
    case "$1" in
    --version) echo "dex2pc ${FAKE_VERSION:-0.2.0}" ;;
    doctor)
        printf '%s\n' "${FAKE_DOCTOR_OUTPUT:-找不到裝置：請連接手機}"
        exit "${FAKE_DOCTOR_STATUS:-1}"
        ;;
    esac
    ;;
podman)
    case "$1" in
    pull) exit "${FAKE_PULL_STATUS:-0}" ;;
    run) exit "${FAKE_RUN_STATUS:-0}" ;;
    esac
    ;;
esac
"#;

const FAKE_NAMES: [&str; 6] = [
    "apt-get",
    "apt-cache",
    "dpkg",
    "equivs-build",
    "dex2pc",
    "podman",
];

/// 放著指向同一支假腳本的連結的目錄。整個測試程式只建一次，
/// 避免多個測試同時寫入並執行同一個檔案（Linux 的 "text file busy"）。
fn fake_bin() -> &'static Path {
    static BIN: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &BIN.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake");
        std::fs::write(&script, FAKE).unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        for name in FAKE_NAMES {
            std::os::unix::fs::symlink(&script, dir.path().join(name)).unwrap();
        }
        let path = dir.path().to_path_buf();
        (dir, path)
    })
    .1
}

fn path_with_fakes() -> std::ffi::OsString {
    let mut paths = vec![fake_bin().to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    std::env::join_paths(paths).unwrap()
}

/// 一次內層腳本的執行：假的容器根目錄、呼叫記錄與 equivs 的 control 副本都放在 `dir`。
struct Run {
    dir: tempfile::TempDir,
    output: Output,
}

impl Run {
    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("log")).unwrap_or_default()
    }

    fn control(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("control")).unwrap_or_default()
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn code(&self) -> Option<i32> {
        self.output.status.code()
    }
}

fn inner(envs: &[(&str, &str)]) -> Run {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(root.join("etc")).unwrap();
    std::fs::write(
        root.join("etc/os-release"),
        "PRETTY_NAME=\"Fake Linux 1\"\n",
    )
    .unwrap();
    let deb = dir.path().join("dex2pc_0.2.0_amd64.deb");
    std::fs::write(&deb, "").unwrap();
    let output = Command::new("sh")
        .arg("packaging/smoke-test-deb-inner.sh")
        .arg("0.2.0")
        .arg(&deb)
        .current_dir(ROOT)
        .env("PATH", path_with_fakes())
        .env("SMOKE_ROOT", &root)
        .env("FAKE_LOG", dir.path().join("log"))
        .env("FAKE_CONTROL_COPY", dir.path().join("control"))
        .envs(envs.iter().copied())
        .output()
        .unwrap();
    Run { dir, output }
}

#[test]
fn inner_passes_with_the_real_scrcpy_when_the_repository_one_is_new_enough() {
    let run = inner(&[("FAKE_SCRCPY_CANDIDATE", "3.3.4-1")]);

    assert_eq!(run.code(), Some(0), "{}", run.stderr());
    assert!(run.stdout().contains("使用真的 scrcpy"), "{}", run.stdout());
    assert!(!run.log().contains("equivs-build"), "{}", run.log());
}

#[test]
fn inner_accepts_a_repository_scrcpy_that_is_exactly_the_minimum() {
    let run = inner(&[("FAKE_SCRCPY_CANDIDATE", "3.2")]);

    assert_eq!(run.code(), Some(0), "{}", run.stderr());
    assert!(!run.log().contains("equivs-build"), "{}", run.log());
}

#[test]
fn inner_reports_the_scrcpy_version_found_in_the_repository() {
    let run = inner(&[("FAKE_SCRCPY_CANDIDATE", "3.3.4-1")]);

    assert!(
        run.stdout().contains("倉庫的 scrcpy 版本：3.3.4-1"),
        "{}",
        run.stdout()
    );
}

#[test]
fn inner_builds_a_labelled_fake_scrcpy_when_the_repository_one_is_too_old() {
    let run = inner(&[("FAKE_SCRCPY_CANDIDATE", "3.1-1")]);

    assert_eq!(run.code(), Some(0), "{}", run.stderr());
    assert!(run.log().contains("equivs-build"), "{}", run.log());
    assert!(
        run.control().contains("Package: scrcpy\n"),
        "{}",
        run.control()
    );
    assert!(
        run.control().contains("Version: 3.2\n"),
        "{}",
        run.control()
    );
    assert!(run.control().contains("FAKE scrcpy"), "{}", run.control());
    assert!(run.stdout().contains("假的"), "{}", run.stdout());
    assert!(
        run.stdout().contains("倉庫的 scrcpy 版本：3.1-1"),
        "{}",
        run.stdout()
    );
}

#[test]
fn inner_builds_the_fake_scrcpy_when_the_repository_has_no_scrcpy() {
    let run = inner(&[("FAKE_SCRCPY_CANDIDATE", "")]);

    assert_eq!(run.code(), Some(0), "{}", run.stderr());
    assert!(run.log().contains("equivs-build"), "{}", run.log());
    assert!(run.stdout().contains("（沒有）"), "{}", run.stdout());
}

#[test]
fn inner_exits_1_when_apt_cache_policy_fails_instead_of_installing_a_fake_scrcpy() {
    let run = inner(&[("FAKE_APT_CACHE_FAIL", "1")]);

    assert_eq!(run.code(), Some(1), "{}", run.stdout());
    assert!(!run.log().contains("equivs-build"), "{}", run.log());
    assert!(
        !run.stdout().contains("smoke test 通過"),
        "{}",
        run.stdout()
    );
}

#[test]
fn inner_installs_the_fake_scrcpy_before_the_dex2pc_deb() {
    let run = inner(&[("FAKE_SCRCPY_CANDIDATE", "")]);

    let log = run.log();
    let fake = log.find("scrcpy_3.2_all.deb").unwrap();
    let dex2pc = log.find("dex2pc_0.2.0_amd64.deb").unwrap();
    assert!(fake < dex2pc, "{log}");
}

#[test]
fn inner_exits_1_when_installed_version_differs_from_expected() {
    let run = inner(&[("FAKE_VERSION", "0.1.0")]);

    assert_eq!(run.code(), Some(1));
    assert!(run.stderr().contains("版本不符"), "{}", run.stderr());
}

#[test]
fn inner_exits_1_when_doctor_exits_with_rust_panic_code_101() {
    let run = inner(&[("FAKE_DOCTOR_STATUS", "101")]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_doctor_exits_zero_as_if_a_phone_were_connected() {
    let run = inner(&[("FAKE_DOCTOR_STATUS", "0")]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_doctor_exit_one_has_no_device_not_found_message() {
    let run = inner(&[("FAKE_DOCTOR_OUTPUT", "something else broke")]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_apt_remove_fails() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "remove"),
        ("FAKE_FAIL_MESSAGE", "E: dpkg was interrupted"),
    ]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_the_binary_is_left_behind_after_remove() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("root");
    std::fs::create_dir_all(root.join("etc")).unwrap();
    std::fs::create_dir_all(root.join("usr/bin")).unwrap();
    std::fs::write(root.join("etc/os-release"), "PRETTY_NAME=\"Fake\"\n").unwrap();
    std::fs::write(root.join("usr/bin/dex2pc"), "").unwrap();
    let deb = dir.path().join("dex2pc.deb");
    std::fs::write(&deb, "").unwrap();

    let output = Command::new("sh")
        .arg("packaging/smoke-test-deb-inner.sh")
        .arg("0.2.0")
        .arg(&deb)
        .current_dir(ROOT)
        .env("PATH", path_with_fakes())
        .env("SMOKE_ROOT", &root)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("/usr/bin/dex2pc"));
}

#[test]
fn inner_exits_77_when_apt_update_cannot_resolve_the_mirror() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "update"),
        (
            "FAKE_FAIL_MESSAGE",
            "W: Failed to fetch http://deb.debian.org/debian/dists/trixie/InRelease  Temporary failure resolving 'deb.debian.org'\nE: Some index files failed to download.",
        ),
    ]);

    assert_eq!(run.code(), Some(77), "{}", run.stderr());
}

#[test]
fn inner_exits_77_when_the_mirror_cannot_be_connected_to() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "update"),
        (
            "FAKE_FAIL_MESSAGE",
            "Err:1 http://archive.ubuntu.com/ubuntu resolute InRelease\n  Could not connect to archive.ubuntu.com:80 (91.189.91.82), connection timed out",
        ),
    ]);

    assert_eq!(run.code(), Some(77), "{}", run.stderr());
}

#[test]
fn inner_exits_77_when_installing_dependencies_hits_http_503() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "install"),
        (
            "FAKE_FAIL_MESSAGE",
            "E: Failed to fetch http://deb.debian.org/debian/pool/main/a/adb.deb  503  Service Unavailable [IP: 151.101.2.132 80]",
        ),
    ]);

    assert_eq!(run.code(), Some(77), "{}", run.stderr());
}

#[test]
fn inner_exits_1_when_the_mirror_answers_404_not_found() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "update"),
        (
            "FAKE_FAIL_MESSAGE",
            "E: Failed to fetch http://deb.debian.org/debian/dists/nope/InRelease  404  Not Found [IP: 151.101.2.132 80]\nE: Some index files failed to download.",
        ),
    ]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_http_404_appears_together_with_a_connection_failure() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "update"),
        (
            "FAKE_FAIL_MESSAGE",
            "Err:1 http://mirror-a/ InRelease  Temporary failure resolving 'mirror-a'\nE: Failed to fetch http://mirror-b/Packages  404  Not Found",
        ),
    ]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_only_a_generic_failed_to_fetch_is_reported() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "update"),
        (
            "FAKE_FAIL_MESSAGE",
            "E: Failed to fetch http://deb.debian.org/debian/dists/trixie/InRelease\nE: Some index files failed to download.",
        ),
    ]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_dex2pc_has_unmet_dependencies() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "install"),
        (
            "FAKE_FAIL_MESSAGE",
            "The following packages have unmet dependencies:\n dex2pc : Depends: scrcpy (>= 3.2) but 3.1-1 is to be installed\nE: Unable to correct problems, you have held broken packages.",
        ),
    ]);

    assert_eq!(run.code(), Some(1));
}

#[test]
fn inner_exits_1_when_a_package_cannot_be_located() {
    let run = inner(&[
        ("FAKE_FAIL_STEP", "equivs"),
        ("FAKE_FAIL_MESSAGE", "E: Unable to locate package equivs"),
        ("FAKE_SCRCPY_CANDIDATE", ""),
    ]);

    assert_eq!(run.code(), Some(1));
}

/// 外層腳本：假的 podman 回報容器內的結果，檢查本機略過、CI 失敗、一般失敗，以及映像名稱與 deb 的處理。
fn outer(
    image: Option<&str>,
    deb: Option<&Path>,
    envs: &[(&str, &str)],
    remove_ci: bool,
) -> Output {
    let mut command = Command::new("sh");
    command.arg("packaging/smoke-test-deb.sh");
    command.args(image);
    command.args(deb);
    command
        .current_dir(ROOT)
        .env("PATH", path_with_fakes())
        .envs(envs.iter().copied());
    if remove_ci {
        // 模擬「開發者本機」，不能讓外層（可能是 CI）的 CI 變數滲進去
        command.env_remove("CI");
    }
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn outer_skips_with_exit_zero_locally_when_container_reports_network_down() {
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = outer(
        Some("debian:13"),
        Some(deb.path()),
        &[("FAKE_RUN_STATUS", "77")],
        true,
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stderr(&output).contains("略過"), "{}", stderr(&output));
}

#[test]
fn outer_fails_in_ci_when_container_reports_network_down() {
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = outer(
        Some("debian:13"),
        Some(deb.path()),
        &[("FAKE_RUN_STATUS", "77"), ("CI", "true")],
        false,
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn outer_keeps_failing_locally_when_container_reports_ordinary_failure() {
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = outer(
        Some("debian:13"),
        Some(deb.path()),
        &[("FAKE_RUN_STATUS", "1")],
        true,
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn outer_skips_locally_when_the_image_cannot_be_pulled() {
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = outer(
        Some("debian:13"),
        Some(deb.path()),
        &[("FAKE_PULL_STATUS", "1")],
        true,
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn outer_fails_in_ci_when_the_image_cannot_be_pulled() {
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = outer(
        Some("debian:13"),
        Some(deb.path()),
        &[("FAKE_PULL_STATUS", "1"), ("CI", "true")],
        false,
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

#[test]
fn outer_pulls_a_short_image_name_from_docker_io_library() {
    let deb = tempfile::NamedTempFile::new().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();

    outer(
        Some("ubuntu:26.04"),
        Some(deb.path()),
        &[("FAKE_LOG", log.path().to_str().unwrap())],
        true,
    );

    let calls = std::fs::read_to_string(log.path()).unwrap();
    assert!(
        calls.contains("podman pull --quiet docker.io/library/ubuntu:26.04"),
        "{calls}"
    );
}

#[test]
fn outer_keeps_a_fully_qualified_image_name_as_given() {
    let deb = tempfile::NamedTempFile::new().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();

    outer(
        Some("registry.example/debian:13"),
        Some(deb.path()),
        &[("FAKE_LOG", log.path().to_str().unwrap())],
        true,
    );

    let calls = std::fs::read_to_string(log.path()).unwrap();
    assert!(
        calls.contains("podman pull --quiet registry.example/debian:13"),
        "{calls}"
    );
}

#[test]
fn outer_script_is_executable_so_the_documented_command_runs_directly() {
    let output = Command::new("packaging/smoke-test-deb.sh")
        .current_dir(ROOT)
        .env_remove("CI")
        .output()
        .unwrap();

    // 沒給映像參數時用法錯誤是 2；沒有執行權限會是 126
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn outer_exits_2_without_an_image_argument() {
    let output = outer(None, None, &[], true);

    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn outer_exits_1_when_the_deb_does_not_exist() {
    let output = outer(
        Some("debian:13"),
        Some(Path::new("/nonexistent/dex2pc.deb")),
        &[],
        true,
    );

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}

/// PATH 只放系統的指令、不含 podman 與 docker 的目錄。
fn path_without_container_engines() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir("/usr/bin").unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name != "podman" && name != "docker" {
            std::os::unix::fs::symlink(entry.path(), dir.path().join(name)).unwrap();
        }
    }
    dir
}

#[test]
fn outer_skips_locally_when_there_is_no_container_engine() {
    let bin = path_without_container_engines();
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = Command::new("/usr/bin/sh")
        .args(["packaging/smoke-test-deb.sh", "debian:13"])
        .arg(deb.path())
        .current_dir(ROOT)
        .env("PATH", bin.path())
        .env_remove("CI")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("podman 或 docker"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn outer_fails_in_ci_when_there_is_no_container_engine() {
    let bin = path_without_container_engines();
    let deb = tempfile::NamedTempFile::new().unwrap();

    let output = Command::new("/usr/bin/sh")
        .args(["packaging/smoke-test-deb.sh", "debian:13"])
        .arg(deb.path())
        .current_dir(ROOT)
        .env("PATH", bin.path())
        .env("CI", "true")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
}
