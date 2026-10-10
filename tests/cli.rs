use std::process::Command;

fn dex2pc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dex2pc"))
}

#[test]
fn version_flag_prints_cargo_toml_version_and_exits_zero() {
    let output = dex2pc().arg("--version").output().unwrap();

    assert!(output.status.success());
    // 預期值取自 Cargo.toml，進版時不用跟著改測試
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!("dex2pc ", env!("CARGO_PKG_VERSION"), "\n")
    );
}

#[test]
fn help_flag_lists_commands_section() {
    let output = dex2pc().arg("--help").output().unwrap();

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Commands:")
    );
}

#[test]
fn doctor_help_lists_serial_option() {
    let output = dex2pc().args(["doctor", "--help"]).output().unwrap();

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("--serial <SERIAL>")
    );
}

#[test]
fn config_path_prints_location_under_xdg_config_home() {
    let dir = tempfile::tempdir().unwrap();

    // 只設定子行程的環境變數，不動測試行程本身的全域環境
    let output = dex2pc()
        .args(["config", "--path"])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{}/dex2pc/config.toml\n", dir.path().display())
    );
}

#[test]
fn config_resolution_flag_overrides_config_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dex2pc")).unwrap();
    std::fs::write(
        dir.path().join("dex2pc/config.toml"),
        "resolution = '2560x1440'\ndpi = 284\n",
    )
    .unwrap();

    let output = dex2pc()
        .args(["config", "--resolution", "3840x2160"])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("resolution = 3840x2160\n"));
    assert!(stdout.contains("dpi = 284\n"));
}

#[test]
fn config_audio_flag_overrides_audio_false_in_config_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dex2pc")).unwrap();
    std::fs::write(dir.path().join("dex2pc/config.toml"), "audio = false\n").unwrap();

    let output = dex2pc()
        .args(["config", "--audio"])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("audio = true\n"));
}

#[test]
fn config_with_unknown_field_exits_non_zero_naming_path_and_field() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dex2pc")).unwrap();
    std::fs::write(dir.path().join("dex2pc/config.toml"), "bitrate = '16M'\n").unwrap();

    let output = dex2pc()
        .arg("config")
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(&format!(
        "設定檔 {}/dex2pc/config.toml 有未知欄位 bitrate",
        dir.path().display()
    )));
}

#[test]
fn config_with_string_field_given_integer_exits_non_zero_naming_path_and_field() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dex2pc")).unwrap();
    std::fs::write(
        dir.path().join("dex2pc/config.toml"),
        "video_bit_rate = 16\n",
    )
    .unwrap();

    let output = dex2pc()
        .arg("config")
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(&format!(
        "設定檔 {}/dex2pc/config.toml 的欄位 video_bit_rate：要是字串",
        dir.path().display()
    )));
}

#[test]
fn config_with_bool_field_given_string_exits_non_zero_naming_path_and_field() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dex2pc")).unwrap();
    std::fs::write(dir.path().join("dex2pc/config.toml"), "audio = \"yes\"\n").unwrap();

    let output = dex2pc()
        .arg("config")
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(&format!(
        "設定檔 {}/dex2pc/config.toml 的欄位 audio：要是 true 或 false",
        dir.path().display()
    )));
}

#[test]
fn config_with_invalid_resolution_flag_is_rejected() {
    let dir = tempfile::tempdir().unwrap();

    let output = dex2pc()
        .args(["config", "--resolution", "0x1080"])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("解析度「0x1080」不合法")
    );
}

#[test]
fn start_dry_run_prints_scrcpy_command_from_config_and_flags() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dex2pc")).unwrap();
    std::fs::write(
        dir.path().join("dex2pc/config.toml"),
        "resolution = '2560x1440'\nvideo_bit_rate = '16M'\n",
    )
    .unwrap();
    // PATH 指向空目錄：若 --dry-run 誤呼叫 adb 或 scrcpy 會找不到程式而失敗
    let empty_path = tempfile::tempdir().unwrap();

    let output = dex2pc()
        .args([
            "start",
            "--dry-run",
            "--serial",
            "R5CW1234ABC",
            "--no-audio",
        ])
        .env("XDG_CONFIG_HOME", dir.path())
        .env("PATH", empty_path.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "scrcpy --serial=R5CW1234ABC --new-display=2560x1440/160 '--window-title=Samsung DeX' --video-bit-rate=16M --no-audio\n"
    );
}

/// F8 結論：實機確認 DeX 不需要改 adb settings，dex2pc 不得在啟動時呼叫 `adb settings`。
/// 假的 adb / scrcpy 把每次呼叫寫進同一份 log；未預期的 adb 呼叫（含 settings）會讓 adb 失敗。
#[test]
fn start_on_supported_device_never_changes_adb_settings() {
    use std::os::unix::fs::PermissionsExt;

    let bin = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let log = bin.path().join("calls.log");
    let log_path = log.display();

    let adb = format!(
        r#"#!/bin/sh
echo "adb $*" >> '{log_path}'
case "$*" in
  version) printf 'Android Debug Bridge version 1.0.41\n' ;;
  "devices -l") printf 'List of devices attached\nR5CW1234ABC  device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\n\n' ;;
  "-s R5CW1234ABC shell getprop") printf '[ro.build.version.oneui]: [80500]\n[ro.build.version.sdk]: [36]\n[ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-S926B]\n' ;;
  *) echo "unexpected adb call" >&2; exit 1 ;;
esac
"#
    );
    let scrcpy = format!(
        r#"#!/bin/sh
echo "scrcpy $*" >> '{log_path}'
case "$*" in
  --version) printf 'scrcpy 3.3.1 <https://github.com/Genymobile/scrcpy>\n' ;;
esac
"#
    );
    for (name, body) in [("adb", adb), ("scrcpy", scrcpy)] {
        let path = bin.path().join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let output = dex2pc()
        .arg("start")
        .env("XDG_CONFIG_HOME", config.path())
        .env("PATH", bin.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        vec![
            "adb version",
            "scrcpy --version",
            "adb devices -l",
            "adb -s R5CW1234ABC shell getprop",
            "scrcpy --serial=R5CW1234ABC --new-display=1920x1080/160 --window-title=Samsung DeX",
        ]
    );
}

#[test]
fn start_without_adb_or_scrcpy_exits_non_zero() {
    let dir = tempfile::tempdir().unwrap();
    let empty_path = tempfile::tempdir().unwrap();

    let output = dex2pc()
        .arg("start")
        .env("XDG_CONFIG_HOME", dir.path())
        .env("PATH", empty_path.path())
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("環境未就緒，未啟動 scrcpy")
    );
}

#[test]
fn install_desktop_writes_entry_execing_this_binary_start() {
    let dir = tempfile::tempdir().unwrap();

    let output = dex2pc()
        .arg("install-desktop")
        .env("XDG_DATA_HOME", dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let entry = std::fs::read_to_string(dir.path().join("applications/dex2pc.desktop")).unwrap();
    assert!(entry.contains(&format!(
        "Exec=\"{}\" start\n",
        env!("CARGO_BIN_EXE_dex2pc")
    )));
}

#[test]
fn install_desktop_uninstall_removes_entry() {
    let dir = tempfile::tempdir().unwrap();
    let installed = dex2pc()
        .arg("install-desktop")
        .env("XDG_DATA_HOME", dir.path())
        .output()
        .unwrap();
    assert!(installed.status.success());

    let output = dex2pc()
        .args(["install-desktop", "--uninstall"])
        .env("XDG_DATA_HOME", dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!dir.path().join("applications/dex2pc.desktop").exists());
}
