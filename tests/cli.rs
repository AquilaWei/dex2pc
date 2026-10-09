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
