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
