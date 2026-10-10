//! packaging/smoke-test-inner.sh 與 smoke-test.sh 的判斷邏輯。
//! 用假的 dnf / dex2pc / podman 取代真實指令，不需要網路、容器或 rpm，所以本機與 CI 都會跑。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// 同一支假指令腳本，依被呼叫的名字（dnf / dex2pc / podman）決定行為，行為由 FAKE_* 環境變數控制。
const FAKE: &str = r#"#!/bin/sh
case "$(basename "$0")" in
dnf)
    step=other
    case "$*" in
    *dnf-plugins-core*) step=plugins ;;
    *copr*) step=copr ;;
    *remove*) step=remove ;;
    *install*) step=install ;;
    esac
    if [ "${FAKE_FAIL_STEP:-}" = "$step" ]; then
        printf '%s\n' "$FAKE_FAIL_MESSAGE" >&2
        exit 1
    fi
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

/// 放著 dnf / dex2pc / podman 三個指向同一支假腳本的連結的目錄。整個測試程式只建一次，
/// 避免多個測試同時寫入並執行同一個檔案（Linux 的 "text file busy"）。
fn fake_bin() -> &'static Path {
    static BIN: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &BIN.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake");
        std::fs::write(&script, FAKE).unwrap();
        std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        for name in ["dnf", "dex2pc", "podman"] {
            std::os::unix::fs::symlink(&script, dir.path().join(name)).unwrap();
        }
        let path = dir.path().to_path_buf();
        (dir, path)
    })
    .1
}

/// 假指令放在 PATH 最前面，其餘（sh、grep、mktemp…）仍用系統的。
fn path_with_fakes() -> std::ffi::OsString {
    let mut paths = vec![fake_bin().to_path_buf()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    std::env::join_paths(paths).unwrap()
}

fn inner(envs: &[(&str, &str)]) -> Output {
    let empty_root = tempfile::tempdir().unwrap();
    inner_with_root(empty_root.path(), envs)
}

/// `root` 代表容器的根目錄，移除後的檢查會看這裡面有沒有殘留檔案。
fn inner_with_root(root: &Path, envs: &[(&str, &str)]) -> Output {
    Command::new("sh")
        .args(["packaging/smoke-test-inner.sh", "0.2.0", "/pkg/dex2pc.rpm"])
        .current_dir(ROOT)
        .env("PATH", path_with_fakes())
        .env("SMOKE_ROOT", root)
        .envs(envs.iter().copied())
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn inner_passes_when_everything_succeeds_and_doctor_reports_no_device() {
    let output = inner(&[]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("== smoke test 通過"));
}

#[test]
fn inner_fails_when_doctor_exits_with_rust_panic_code_101() {
    let output = inner(&[("FAKE_DOCTOR_STATUS", "101")]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("doctor 退出碼應為 1，實際是 101"));
}

#[test]
fn inner_fails_when_doctor_exits_zero_as_if_a_phone_were_connected() {
    let output = inner(&[("FAKE_DOCTOR_STATUS", "0")]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn inner_fails_when_doctor_is_killed_by_signal_exit_code_139() {
    let output = inner(&[("FAKE_DOCTOR_STATUS", "139")]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn inner_fails_when_doctor_exit_one_has_no_device_not_found_message() {
    let output = inner(&[("FAKE_DOCTOR_OUTPUT", "adb：缺少")]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("沒有回報「找不到裝置」"));
}

#[test]
fn inner_fails_when_installed_version_differs_from_expected() {
    let output = inner(&[("FAKE_VERSION", "9.9.9")]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn inner_exits_77_when_copr_enable_cannot_resolve_host() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "copr"),
        (
            "FAKE_FAIL_MESSAGE",
            "Curl error (6): Could not resolve host: copr.fedorainfracloud.org",
        ),
    ]);

    assert_eq!(output.status.code(), Some(77), "{output:?}");
}

#[test]
fn inner_exits_77_when_rpm_install_cannot_download_dependencies_from_copr() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "install"),
        (
            "FAKE_FAIL_MESSAGE",
            "Failed to download metadata for repo 'copr:copr.fedorainfracloud.org:zeno:scrcpy'",
        ),
    ]);

    assert_eq!(output.status.code(), Some(77), "{output:?}");
}

#[test]
fn inner_exits_1_when_rpm_install_fails_for_a_non_network_reason() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "install"),
        (
            "FAKE_FAIL_MESSAGE",
            "Error: Problem: conflicting requests - nothing provides scrcpy >= 3.2",
        ),
    ]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("nothing provides scrcpy"));
}

#[test]
fn inner_exits_1_when_dnf5_cannot_resolve_transaction_for_a_missing_dependency() {
    // Fedora 44 的 dnf5 對「依賴裝不上」印的是這段（不是網路問題），要當成一般失敗
    let output = inner(&[
        ("FAKE_FAIL_STEP", "install"),
        (
            "FAKE_FAIL_MESSAGE",
            "Failed to resolve the transaction:\nNo match for argument: scrcpy",
        ),
    ]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn inner_exits_1_when_dnf_remove_fails() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "remove"),
        ("FAKE_FAIL_MESSAGE", "Error: something broke"),
    ]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn inner_fails_when_binary_is_left_behind_after_remove() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("usr/bin")).unwrap();
    std::fs::write(root.path().join("usr/bin/dex2pc"), b"").unwrap();

    let output = inner_with_root(root.path(), &[]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("移除後 /usr/bin/dex2pc 還在"));
}

#[test]
fn inner_exits_1_when_copr_enable_fails_for_a_non_network_reason() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "copr"),
        ("FAKE_FAIL_MESSAGE", "Error: unknown argument 'enable'"),
    ]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// 外層腳本：假的 podman 回報容器內的結果，檢查本機略過、CI 失敗、一般失敗三種處理。
fn outer(envs: &[(&str, &str)], remove_ci: bool) -> Output {
    let rpm = tempfile::NamedTempFile::new().unwrap();
    let mut command = Command::new("sh");
    command
        .args(["packaging/smoke-test.sh"])
        .arg(rpm.path())
        .current_dir(ROOT)
        .env("PATH", path_with_fakes())
        .envs(envs.iter().copied());
    if remove_ci {
        // 模擬「開發者本機」，不能讓外層（可能是 CI）的 CI 變數滲進去
        command.env_remove("CI");
    }
    command.output().unwrap()
}

#[test]
fn outer_skips_with_exit_zero_locally_when_container_reports_network_down() {
    let output = outer(&[("FAKE_RUN_STATUS", "77")], true);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(stderr(&output).contains("略過：容器內連不到 COPR"));
}

#[test]
fn outer_fails_in_ci_when_container_reports_network_down() {
    let output = outer(&[("FAKE_RUN_STATUS", "77"), ("CI", "1")], false);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("CI 不可略過"));
}

#[test]
fn outer_keeps_failing_locally_when_container_reports_ordinary_failure() {
    let output = outer(&[("FAKE_RUN_STATUS", "1")], true);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn outer_skips_locally_when_image_cannot_be_pulled() {
    let output = outer(&[("FAKE_PULL_STATUS", "1")], true);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(stderr(&output).contains("略過：拉不到映像"));
}

#[test]
fn inner_exits_1_when_copr_returns_http_404_for_a_mistyped_repo() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "copr"),
        (
            "FAKE_FAIL_MESSAGE",
            "Status code: 404 for https://copr.fedorainfracloud.org/coprs/zeno/scrcpyy/repo/fedora-44/",
        ),
    ]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

#[test]
fn inner_exits_77_when_copr_server_answers_http_503() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "copr"),
        (
            "FAKE_FAIL_MESSAGE",
            "Status code: 503 for https://copr.fedorainfracloud.org/coprs/zeno/scrcpy/repo/fedora-44/",
        ),
    ]);

    assert_eq!(output.status.code(), Some(77), "{output:?}");
}

#[test]
fn inner_exits_77_when_copr_connection_is_unreachable() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "copr"),
        (
            "FAKE_FAIL_MESSAGE",
            "copr.fedorainfracloud.org: Network is unreachable",
        ),
    ]);

    assert_eq!(output.status.code(), Some(77), "{output:?}");
}

#[test]
fn inner_exits_77_when_copr_connection_times_out() {
    let output = inner(&[
        ("FAKE_FAIL_STEP", "copr"),
        (
            "FAKE_FAIL_MESSAGE",
            "copr.fedorainfracloud.org: Connection timed out after 30001 milliseconds",
        ),
    ]);

    assert_eq!(output.status.code(), Some(77), "{output:?}");
}
