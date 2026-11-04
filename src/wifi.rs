use std::fs;
use std::io::{self, Write};
use std::net::Ipv4Addr;
use std::path::Path;

use crate::device;
use crate::runner::CommandRunner;

/// 僅接受 IPv4 或 ASCII DNS 主機名，加十進位 1–65535 埠號；不接受 IPv6。
/// 不解析 DNS，避免驗證依賴網路；空白、選項、無效標籤或數字 IPv4 都會失敗。
pub fn validate(address: &str) -> Result<String, String> {
    let invalid =
        || format!("無效位址 {address:?}：請使用 IPv4 或主機名:埠號（1–65535，不支援 IPv6）");
    let (host, port) = address.split_once(':').ok_or_else(invalid)?;
    if port.is_empty()
        || !port.bytes().all(|b| b.is_ascii_digit())
        || port.parse::<u16>().ok().is_none_or(|p| p == 0)
    {
        return Err(invalid());
    }
    let valid_host = if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        host.parse::<Ipv4Addr>().is_ok()
    } else {
        !host.is_empty()
            && host.len() <= 253
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.as_bytes()[0].is_ascii_alphanumeric()
                    && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
    };
    if !valid_host {
        return Err(invalid());
    }
    Ok(address.to_string())
}

/// 配對碼交由 adb 從終端互動讀取；位址無效或程序啟動失敗時不繼續。
/// 沿用 adb 的退出碼，訊號中止則回傳 1。
pub fn pair(runner: &dyn CommandRunner, address: &str) -> io::Result<u8> {
    validate(address).map_err(io::Error::other)?;
    Ok(runner
        .interactive("adb", &["pair", address])?
        .and_then(|c| u8::try_from(c).ok())
        .unwrap_or(1))
}

/// 保留中斷狀態，讓 watch 在 Ctrl+C 後停止而非再試。
#[derive(Debug, PartialEq, Eq)]
pub enum Connection {
    Connected,
    Failed,
    Interrupted,
}

/// adb 有時以 exit code 0 回報連線失敗，因此還要確認文字含 connected to。
/// 命令錯誤寫入 out，只有輸出寫入失敗才回傳 I/O 錯誤。
pub fn connect(
    runner: &dyn CommandRunner,
    address: &str,
    out: &mut dyn Write,
) -> io::Result<Connection> {
    validate(address).map_err(io::Error::other)?;
    match runner.run("adb", &["connect", address]) {
        Ok(output) => {
            writeln!(out, "{}{}", output.stdout, output.stderr)?;
            if output.code.is_none() {
                return Ok(Connection::Interrupted);
            }
            if output.success()
                && (output.stdout.contains("connected to")
                    || output.stderr.contains("connected to"))
            {
                Ok(Connection::Connected)
            } else {
                writeln!(out, "Wi-Fi 連線失敗：{address}")?;
                Ok(Connection::Failed)
            }
        }
        Err(err) => {
            writeln!(out, "無法執行 adb connect：{err}")?;
            Ok(Connection::Failed)
        }
    }
}

/// 先完成連線才改設定，避免把不可用的位址寫入；保留原 TOML 欄位但不保留註解。
/// USB 模式只選非網路序號；命令、裝置選擇、無 WLAN IP 或設定檔讀寫失敗皆回傳錯誤。
pub fn connect_and_save(
    runner: &dyn CommandRunner,
    address: Option<&str>,
    serial: Option<&str>,
    path: &Path,
    out: &mut dyn Write,
) -> io::Result<()> {
    // 先驗證原設定，避免覆寫損壞的檔案。
    crate::config::load(path).map_err(io::Error::other)?;
    let address = match address {
        Some(address) => validate(address).map_err(io::Error::other)?,
        None => usb_address(runner, serial)?,
    };
    if connect(runner, &address, out)? != Connection::Connected {
        return Err(io::Error::other("未建立 Wi-Fi 連線，設定未儲存"));
    }
    save(path, &address)?;
    writeln!(out, "已儲存 wifi_address = {address}")
}

fn checked(runner: &dyn CommandRunner, args: &[&str]) -> io::Result<String> {
    let output = runner.run("adb", args)?;
    if !output.success() {
        return Err(io::Error::other(format!(
            "adb {} 失敗：{}{}",
            args.join(" "),
            output.stdout,
            output.stderr
        )));
    }
    Ok(output.stdout)
}

fn usb_address(runner: &dyn CommandRunner, serial: Option<&str>) -> io::Result<String> {
    let listed = checked(runner, &["devices", "-l"])?;
    let devices: Vec<_> = device::parse_devices(&listed)
        .into_iter()
        .filter(|d| !d.serial.contains(':') && !d.serial.starts_with("adb-"))
        .collect();
    if devices.is_empty() {
        return Err(io::Error::other(
            "找不到 USB 手機：請開啟「無線偵錯」，使用 dex2pc pair IP:PORT，再執行 dex2pc connect IP:PORT",
        ));
    }
    let selected = device::select(&devices, serial).map_err(io::Error::other)?;
    let serial = &selected.serial;
    checked(runner, &["-s", serial, "tcpip", "5555"])?;
    let ip = checked(
        runner,
        &[
            "-s", serial, "shell", "ip", "-f", "inet", "addr", "show", "wlan0",
        ],
    )?;
    let ip = wlan_ip(&ip).ok_or_else(|| {
        io::Error::other("讀不到手機 wlan0 的 IPv4 位址：請開啟 Wi-Fi 並連上與電腦相通的網路後重試")
    })?;
    Ok(format!("{ip}:5555"))
}

fn wlan_ip(output: &str) -> Option<Ipv4Addr> {
    let mut words = output.split_whitespace();
    while let Some(word) = words.next() {
        if word == "inet"
            && let Some(ip) = words
                .next()
                .and_then(|s| s.split_once('/'))
                .and_then(|(ip, _)| ip.parse().ok())
        {
            return Some(ip);
        }
    }
    None
}

fn save(path: &Path, address: &str) -> io::Result<()> {
    let mut table: toml::Table = match fs::read_to_string(path) {
        Ok(text) => text.parse().map_err(io::Error::other)?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => toml::Table::new(),
        Err(err) => return Err(err),
    };
    table.insert(
        "wifi_address".to_string(),
        toml::Value::String(address.to_string()),
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string(&table).map_err(io::Error::other)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandOutput;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct FakeRunner {
        outputs: RefCell<VecDeque<CommandOutput>>,
        calls: RefCell<Vec<String>>,
    }
    impl FakeRunner {
        fn new(outputs: &[(&str, Option<i32>)]) -> Self {
            Self {
                outputs: RefCell::new(
                    outputs
                        .iter()
                        .map(|(text, code)| CommandOutput {
                            code: *code,
                            stdout: (*text).into(),
                            stderr: String::new(),
                        })
                        .collect(),
                ),
                calls: RefCell::default(),
            }
        }
    }
    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            self.calls
                .borrow_mut()
                .push(format!("{program} {}", args.join(" ")));
            Ok(self.outputs.borrow_mut().pop_front().expect("非預期呼叫"))
        }
        fn interactive(&self, program: &str, args: &[&str]) -> io::Result<Option<i32>> {
            assert_eq!(program, "adb");
            assert_eq!(args, ["pair", "phone.local:37001"]);
            Ok(Some(7))
        }
    }
    #[test]
    fn pair_with_valid_address_returns_interactive_exit_code() {
        assert_eq!(pair(&FakeRunner::new(&[]), "phone.local:37001").unwrap(), 7);
    }
    #[test]
    fn pair_with_invalid_address_does_not_execute_adb() {
        let runner = FakeRunner::new(&[]);
        assert!(pair(&runner, "-phone:5555").is_err());
        assert!(runner.calls.borrow().is_empty());
    }
    #[test]
    fn numeric_ipv4_with_invalid_octet_is_rejected() {
        assert!(validate("192.168.1.999:5555").is_err());
    }
    #[test]
    fn port_zero_is_rejected() {
        assert!(validate("phone:0").is_err());
    }
    #[test]
    fn port_overflow_is_rejected() {
        assert!(validate("phone:65536").is_err());
    }
    #[test]
    fn ipv6_is_rejected() {
        assert!(validate("[::1]:5555").is_err());
    }
    #[test]
    fn whitespace_is_rejected() {
        assert!(validate("phone name:5555").is_err());
    }
    #[test]
    fn valid_dns_with_maximum_port_is_accepted() {
        assert_eq!(
            validate("phone-1.local:65535").unwrap(),
            "phone-1.local:65535"
        );
    }
    #[test]
    fn connect_with_already_connected_text_succeeds() {
        assert_eq!(
            connect(
                &FakeRunner::new(&[("already connected to phone:5555", Some(0))]),
                "phone:5555",
                &mut Vec::new()
            )
            .unwrap(),
            Connection::Connected
        );
    }
    #[test]
    fn connect_with_failure_text_and_zero_exit_fails() {
        let mut out = Vec::new();
        assert_eq!(
            connect(
                &FakeRunner::new(&[("failed to connect: refused", Some(0))]),
                "phone:5555",
                &mut out
            )
            .unwrap(),
            Connection::Failed
        );
        assert!(String::from_utf8(out).unwrap().contains("refused"));
    }
    #[test]
    fn connect_interrupted_preserves_interrupt() {
        assert_eq!(
            connect(
                &FakeRunner::new(&[("", None)]),
                "phone:5555",
                &mut Vec::new()
            )
            .unwrap(),
            Connection::Interrupted
        );
    }
    #[test]
    fn connect_success_creates_config_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dex2pc/config.toml");
        connect_and_save(
            &FakeRunner::new(&[("connected to phone:5555", Some(0))]),
            Some("phone:5555"),
            None,
            &path,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "wifi_address = \"phone:5555\"\n"
        );
    }
    #[test]
    fn connect_success_preserves_other_config_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "dpi = 284\n").unwrap();
        connect_and_save(
            &FakeRunner::new(&[("connected to phone:5555", Some(0))]),
            Some("phone:5555"),
            None,
            &path,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "dpi = 284\nwifi_address = \"phone:5555\"\n"
        );
    }
    #[test]
    fn connect_failure_does_not_save_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert!(
            connect_and_save(
                &FakeRunner::new(&[("cannot connect", Some(0))]),
                Some("phone:5555"),
                None,
                &path,
                &mut Vec::new()
            )
            .is_err()
        );
        assert!(!path.exists());
    }
    #[test]
    fn usb_connect_switches_transport_reads_wlan_and_connects() {
        let runner = FakeRunner::new(&[
            ("USB device\nphone:5555 device\n", Some(0)),
            ("restarting", Some(0)),
            ("inet 192.168.1.20/24 brd 192.168.1.255", Some(0)),
            ("connected to 192.168.1.20:5555", Some(0)),
        ]);
        let dir = tempfile::tempdir().unwrap();
        connect_and_save(
            &runner,
            None,
            None,
            &dir.path().join("config.toml"),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb devices -l",
                "adb -s USB tcpip 5555",
                "adb -s USB shell ip -f inet addr show wlan0",
                "adb connect 192.168.1.20:5555"
            ]
        );
    }
    #[test]
    fn usb_without_wlan_ip_explains_wifi_requirement() {
        let runner = FakeRunner::new(&[("USB device", Some(0)), ("", Some(0)), ("", Some(0))]);
        assert!(
            usb_address(&runner, None)
                .unwrap_err()
                .to_string()
                .contains("請開啟 Wi-Fi")
        );
    }
    #[test]
    fn no_usb_device_prints_wireless_debugging_hint() {
        let runner = FakeRunner::new(&[("phone:5555 device", Some(0))]);
        assert!(
            usb_address(&runner, None)
                .unwrap_err()
                .to_string()
                .contains("dex2pc pair IP:PORT")
        );
    }
    #[test]
    fn multiple_usb_devices_require_serial() {
        let runner = FakeRunner::new(&[("USB1 device\nUSB2 device", Some(0))]);
        assert!(
            usb_address(&runner, None)
                .unwrap_err()
                .to_string()
                .contains("--serial")
        );
    }
    #[test]
    fn serial_selects_usb_device_before_switching() {
        let runner = FakeRunner::new(&[
            ("USB1 device\nUSB2 device", Some(0)),
            ("", Some(0)),
            ("inet 192.168.1.20/24", Some(0)),
        ]);
        assert_eq!(
            usb_address(&runner, Some("USB2")).unwrap(),
            "192.168.1.20:5555"
        );
        assert_eq!(runner.calls.borrow()[1], "adb -s USB2 tcpip 5555");
    }
}
