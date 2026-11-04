use std::fs;
use std::io::{self, Write};
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

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
pub fn pair(runner: &dyn CommandRunner, address: &str, out: &mut dyn Write) -> io::Result<u8> {
    validate(address).map_err(io::Error::other)?;
    let code = runner.interactive("adb", &["pair", address])?;
    // status 等 adb 印完自己的錯誤才返回，提示不會蓋過原始原因。
    if code.is_some_and(|code| code != 0) {
        writeln!(
            out,
            "配對失敗：請重新開啟手機的配對視窗，使用新的配對碼與埠號；執行 adb kill-server 後重試。Fedora／發行版 adb 有已知配對失敗（protocol fault (couldn't read status message)），可改用 Google 官方 platform-tools，或接 USB 執行 dex2pc connect 跳過配對。"
        )?;
    }
    Ok(code.and_then(|c| u8::try_from(c).ok()).unwrap_or(1))
}

/// 保留中斷狀態，讓 watch 在 Ctrl+C 後停止而非再試。
#[derive(Debug, PartialEq, Eq)]
pub enum Connection {
    Connected,
    Failed,
    Interrupted,
}

/// 啟動投影前清除失效的 TCP transport，避免 adb 誤報已連線而讓後續 shell 卡住。
/// disconnect 失敗不影響後續連線；中斷則立即停止，輸出寫入失敗往上傳。
pub fn fresh_connect(
    runner: &dyn CommandRunner,
    address: &str,
    out: &mut dyn Write,
) -> io::Result<Connection> {
    validate(address).map_err(io::Error::other)?;
    // 網路斷線後 adb 可能保留舊 TCP transport；必須先清除才能真正建立新連線。
    if let Ok(output) = runner.run("adb", &["disconnect", address])
        && output.code.is_none()
    {
        return Ok(Connection::Interrupted);
    }
    connect(runner, address, out)
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
            for message in [&output.stdout, &output.stderr] {
                let message = message.trim();
                if !message.is_empty() {
                    writeln!(out, "{message}")?;
                }
            }
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
/// USB 模式只選非網路序號並在重啟 adbd 後短暫重試；sleep 由呼叫端注入。
/// 命令、裝置選擇、無 WLAN IP 或設定檔讀寫失敗皆回傳錯誤。
pub fn connect_and_save(
    runner: &dyn CommandRunner,
    address: Option<&str>,
    serial: Option<&str>,
    path: &Path,
    sleep: &mut dyn FnMut(Duration),
    out: &mut dyn Write,
) -> io::Result<()> {
    // 先驗證原設定，避免覆寫損壞的檔案。
    crate::config::load(path).map_err(io::Error::other)?;
    let from_usb = address.is_none();
    let address = match address {
        Some(address) => validate(address).map_err(io::Error::other)?,
        None => usb_address(runner, serial)?,
    };
    let connection = if from_usb {
        connect_after_usb(runner, &address, sleep, out)?
    } else {
        connect(runner, &address, out)?
    };
    if connection != Connection::Connected {
        return Err(io::Error::other("未建立 Wi-Fi 連線，設定未儲存"));
    }
    save(path, &address)?;
    writeln!(out, "已儲存 wifi_address = {address}")
}

/// tcpip 會重啟 adbd，TCP 監聽可能尚未就緒；僅 USB 切換最多嘗試三次，間隔一秒。
/// 注入 sleep 讓測試能核對等待，不真的延遲；中斷立即停止，輸出錯誤往上傳。
fn connect_after_usb(
    runner: &dyn CommandRunner,
    address: &str,
    sleep: &mut dyn FnMut(Duration),
    out: &mut dyn Write,
) -> io::Result<Connection> {
    for attempt in 0..3 {
        if attempt > 0 {
            sleep(Duration::from_secs(1));
        }
        let result = connect(runner, address, out)?;
        if result != Connection::Failed {
            return Ok(result);
        }
    }
    Ok(Connection::Failed)
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
    // 必須先讀 IP：tcpip 會重啟 adbd，緊接著用舊 USB transport 下 shell 可能得到 error: closed。
    let ip = checked(
        runner,
        &[
            "-s", serial, "shell", "ip", "-f", "inet", "addr", "show", "wlan0",
        ],
    )?;
    let ip = wlan_ip(&ip).ok_or_else(|| {
        io::Error::other("讀不到手機 wlan0 的 IPv4 位址：請開啟 Wi-Fi 並連上與電腦相通的網路後重試")
    })?;
    checked(runner, &["-s", serial, "tcpip", "5555"])?;
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
        assert_eq!(
            pair(&FakeRunner::new(&[]), "phone.local:37001", &mut Vec::new()).unwrap(),
            7
        );
    }
    #[test]
    fn pair_with_invalid_address_does_not_execute_adb() {
        let runner = FakeRunner::new(&[]);
        assert!(pair(&runner, "-phone:5555", &mut Vec::new()).is_err());
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
            &mut |_| panic!("不應等待"),
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
            &mut |_| panic!("不應等待"),
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
                &mut |_| panic!("不應等待"),
                &mut Vec::new()
            )
            .is_err()
        );
        assert!(!path.exists());
    }
    #[test]
    fn usb_switch_reads_ip_before_restarting_adbd() {
        let runner = FakeRunner::new(&[
            ("USB device", Some(0)),
            ("inet 192.168.1.20/24", Some(0)),
            ("inet 192.168.1.20/24", Some(0)),
        ]);
        usb_address(&runner, None).unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb devices -l",
                "adb -s USB shell ip -f inet addr show wlan0",
                "adb -s USB tcpip 5555",
            ]
        );
    }
    #[test]
    fn usb_connect_reads_wlan_switches_transport_and_connects() {
        let runner = FakeRunner::new(&[
            ("USB device\nphone:5555 device\n", Some(0)),
            ("inet 192.168.1.20/24 brd 192.168.1.255", Some(0)),
            ("restarting", Some(0)),
            ("connected to 192.168.1.20:5555", Some(0)),
        ]);
        let dir = tempfile::tempdir().unwrap();
        connect_and_save(
            &runner,
            None,
            None,
            &dir.path().join("config.toml"),
            &mut |_| panic!("不應等待"),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb devices -l",
                "adb -s USB shell ip -f inet addr show wlan0",
                "adb -s USB tcpip 5555",
                "adb connect 192.168.1.20:5555"
            ]
        );
    }
    #[test]
    fn usb_without_wlan_ip_explains_wifi_requirement_without_running_tcpip() {
        let runner = FakeRunner::new(&[("USB device", Some(0)), ("", Some(0))]);
        assert!(
            usb_address(&runner, None)
                .unwrap_err()
                .to_string()
                .contains("請開啟 Wi-Fi")
        );
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb devices -l",
                "adb -s USB shell ip -f inet addr show wlan0"
            ]
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
            ("inet 192.168.1.20/24", Some(0)),
            ("", Some(0)),
        ]);
        assert_eq!(
            usb_address(&runner, Some("USB2")).unwrap(),
            "192.168.1.20:5555"
        );
        assert_eq!(
            runner.calls.borrow()[1],
            "adb -s USB2 shell ip -f inet addr show wlan0"
        );
    }
    #[test]
    fn pair_nonzero_exit_prints_recovery_hint() {
        let mut out = Vec::new();
        pair(&FakeRunner::new(&[]), "phone.local:37001", &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "配對失敗：請重新開啟手機的配對視窗，使用新的配對碼與埠號；執行 adb kill-server 後重試。Fedora／發行版 adb 有已知配對失敗（protocol fault (couldn't read status message)），可改用 Google 官方 platform-tools，或接 USB 執行 dex2pc connect 跳過配對。\n"
        );
    }

    #[test]
    fn usb_switch_retries_refused_connection_then_saves_after_success() {
        let runner = FakeRunner::new(&[
            ("USB device", Some(0)),
            ("inet 192.168.1.20/24", Some(0)),
            ("restarting", Some(0)),
            ("failed to connect: connection refused", Some(0)),
            ("connected to 192.168.1.20:5555", Some(0)),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut delays = Vec::new();
        connect_and_save(
            &runner,
            None,
            None,
            &path,
            &mut |delay| delays.push(delay),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb devices -l",
                "adb -s USB shell ip -f inet addr show wlan0",
                "adb -s USB tcpip 5555",
                "adb connect 192.168.1.20:5555",
                "adb connect 192.168.1.20:5555",
            ]
        );
        assert_eq!(delays, [Duration::from_secs(1)]);
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "wifi_address = \"192.168.1.20:5555\"\n"
        );
    }

    #[test]
    fn usb_switch_gives_up_after_three_connect_attempts_without_saving() {
        let runner = FakeRunner::new(&[
            ("USB device", Some(0)),
            ("inet 192.168.1.20/24", Some(0)),
            ("restarting", Some(0)),
            ("failed to connect", Some(0)),
            ("connection refused", Some(1)),
            ("connection refused", Some(1)),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut delays = Vec::new();
        assert!(
            connect_and_save(
                &runner,
                None,
                None,
                &path,
                &mut |delay| delays.push(delay),
                &mut Vec::new()
            )
            .is_err()
        );
        assert_eq!(
            *runner.calls.borrow(),
            [
                "adb devices -l",
                "adb -s USB shell ip -f inet addr show wlan0",
                "adb -s USB tcpip 5555",
                "adb connect 192.168.1.20:5555",
                "adb connect 192.168.1.20:5555",
                "adb connect 192.168.1.20:5555",
            ]
        );
        assert_eq!(delays, [Duration::from_secs(1), Duration::from_secs(1)]);
        assert!(!path.exists());
    }

    #[test]
    fn usb_switch_interrupted_connect_does_not_retry() {
        let runner = FakeRunner::new(&[("", None)]);
        assert_eq!(
            connect_after_usb(
                &runner,
                "phone:5555",
                &mut |_| panic!("不應等待"),
                &mut Vec::new()
            )
            .unwrap(),
            Connection::Interrupted
        );
        assert_eq!(*runner.calls.borrow(), ["adb connect phone:5555"]);
    }
    #[test]
    fn fresh_connection_ignores_nonzero_disconnect_status() {
        let runner = FakeRunner::new(&[
            ("disconnect failed", Some(1)),
            ("connected to phone:5555", Some(0)),
        ]);
        assert_eq!(
            fresh_connect(&runner, "phone:5555", &mut Vec::new()).unwrap(),
            Connection::Connected
        );
        assert_eq!(
            *runner.calls.borrow(),
            ["adb disconnect phone:5555", "adb connect phone:5555"]
        );
    }
    #[test]
    fn fresh_connection_ignores_disconnect_io_error() {
        struct DisconnectUnavailable;
        impl CommandRunner for DisconnectUnavailable {
            fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
                assert_eq!(program, "adb");
                match args {
                    ["disconnect", "phone:5555"] => Err(io::Error::other("無法執行 disconnect")),
                    ["connect", "phone:5555"] => Ok(CommandOutput {
                        code: Some(0),
                        stdout: "connected to phone:5555".into(),
                        stderr: String::new(),
                    }),
                    _ => panic!("非預期呼叫"),
                }
            }
        }
        assert_eq!(
            fresh_connect(&DisconnectUnavailable, "phone:5555", &mut Vec::new()).unwrap(),
            Connection::Connected
        );
    }
    #[test]
    fn connect_trims_stdout_and_stderr_without_extra_blank_lines() {
        let runner = FakeRunner {
            outputs: RefCell::new(VecDeque::from([CommandOutput {
                code: Some(0),
                stdout: "  connected to phone:5555\n\n".into(),
                stderr: "  診斷訊息\n".into(),
            }])),
            calls: RefCell::default(),
        };
        let mut out = Vec::new();
        connect(&runner, "phone:5555", &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "connected to phone:5555\n診斷訊息\n"
        );
    }
    #[test]
    fn connect_does_not_print_whitespace_only_output() {
        let runner = FakeRunner::new(&[(" \n\t", None)]);
        let mut out = Vec::new();
        connect(&runner, "phone:5555", &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "");
    }
}
