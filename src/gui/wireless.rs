use std::io::{self, Read};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crate::runner::{CommandRunner, TimedOutput};

pub(super) const NOT_FOUND: &str = super::text::get(
    crate::preferences::Language::ZhTw,
    super::text::Key::NotFound,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Pair,
    Connect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Service {
    pub name: String,
    pub kind: Kind,
    pub address: String,
}

pub(super) fn parse(output: &str) -> Vec<Service> {
    output
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let kind = match words.next()?.trim_end_matches('.') {
                "_adb-tls-pairing._tcp" => Kind::Pair,
                "_adb-tls-connect._tcp" => Kind::Connect,
                _ => return None,
            };
            Some(Service {
                name: name.into(),
                kind,
                address: crate::wifi::validate(words.next()?).ok()?,
            })
        })
        .collect()
}

fn identity(name: &str) -> &str {
    name.strip_prefix("adb-")
        .unwrap_or(name)
        .rsplit_once('-')
        .map_or(name, |(id, _)| id)
}

pub(super) fn choose(
    services: &[Service],
    saved: Option<&str>,
    device: Option<&str>,
) -> Option<Service> {
    let entries: Vec<_> = services
        .iter()
        .filter(|s| s.kind == Kind::Connect)
        .collect();
    let device_match = device.and_then(|device| {
        entries
            .iter()
            .find(|s| identity(&s.name) == identity(device))
    });
    let ip_match = super::input::host(saved).and_then(|host| {
        entries
            .iter()
            .find(|s| super::input::host(Some(&s.address)) == Some(host))
    });
    device_match
        .or(ip_match)
        .copied()
        .or_else(|| (entries.len() == 1).then(|| entries[0]))
        .cloned()
}

pub(super) struct Pairing {
    pub name: String,
    password: String,
    pub qr: qrcode::QrCode,
}

pub(super) fn payload(name: &str, password: &str) -> String {
    format!("WIFI:T:ADB;S:{name};P:{password};;")
}

impl Pairing {
    pub fn new() -> io::Result<Self> {
        // OS entropy, not timestamps; hexadecimal avoids QR Wi-Fi escaping characters.
        let mut bytes = [0_u8; 24];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let name = format!("studio-{}", hex(&bytes[..8]));
        let password = hex(&bytes[8..]);
        let qr = qrcode::QrCode::new(payload(&name, &password)).map_err(|_| {
            io::Error::other(super::text::get(
                crate::preferences::Language::ZhTw,
                super::text::Key::QrFailed,
            ))
        })?;
        Ok(Self { name, password, qr })
    }
    pub fn password(&self) -> String {
        self.password.clone()
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").expect("String write");
        text
    })
}

pub(super) fn services(runner: &dyn CommandRunner) -> Result<Vec<Service>, String> {
    match runner.run_timeout("adb", &["mdns", "services"], Duration::from_secs(2)) {
        Ok(TimedOutput::Exited(output)) if output.success() => Ok(parse(&output.stdout)),
        _ => Err(super::text::get(
            crate::preferences::Language::ZhTw,
            super::text::Key::ScanFailed,
        )
        .into()),
    }
}

// 5 connect scans: at most 12 seconds including command deadlines and short waits.
// QR waits longer for the user to scan, but cancellation wakes the wait immediately.
pub(super) fn discover(
    runner: &dyn CommandRunner,
    cancel: &Receiver<()>,
    attempts: usize,
    select: impl Fn(&[Service]) -> Option<Service>,
) -> Result<Service, String> {
    for attempt in 0..attempts {
        if cancel.try_recv().is_ok() {
            return Err(super::text::zh(super::text::Key::Cancelled).into());
        }
        if let Some(service) = select(&services(runner)?) {
            return Ok(service);
        }
        if attempt + 1 < attempts && cancel.recv_timeout(Duration::from_millis(500)).is_ok() {
            return Err(super::text::zh(super::text::Key::Cancelled).into());
        }
    }
    Err(NOT_FOUND.into())
}

pub(super) fn pair_qr(
    runner: &dyn CommandRunner,
    address: &str,
    password: &str,
) -> Result<(), String> {
    // Discard adb output AND I/O errors: either may echo credentials.
    match runner.run_timeout("adb", &["pair", address, password], Duration::from_secs(15)) {
        Ok(TimedOutput::Exited(output))
            if output.success() && output.stdout.contains("Successfully paired") =>
        {
            Ok(())
        }
        _ => Err(super::text::get(
            crate::preferences::Language::ZhTw,
            super::text::Key::PairFailed,
        )
        .into()),
    }
}

pub(super) fn first_run(options: &crate::scrcpy::Options) -> bool {
    options.mode.is_none()
        || (options.mode == Some(crate::preferences::Mode::Wifi) && options.wifi_address.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::CommandOutput;
    struct Fake(&'static str);
    impl CommandRunner for Fake {
        fn run(&self, _: &str, _: &[&str]) -> io::Result<CommandOutput> {
            panic!("timeout required")
        }
        fn run_timeout(
            &self,
            program: &str,
            args: &[&str],
            timeout: Duration,
        ) -> io::Result<TimedOutput> {
            assert_eq!(
                (program, args, timeout),
                (
                    "adb",
                    ["mdns", "services"].as_slice(),
                    Duration::from_secs(2)
                )
            );
            Ok(TimedOutput::Exited(CommandOutput {
                code: Some(0),
                stdout: self.0.into(),
                stderr: String::new(),
            }))
        }
    }
    #[test]
    fn parses_pairing_service() {
        assert_eq!(services(&Fake("List of discovered mdns services\nstudio-test _adb-tls-pairing._tcp 192.168.18.59:44517\n")).unwrap(), vec![Service { name: "studio-test".into(), kind: Kind::Pair, address: "192.168.18.59:44517".into() }]);
    }
    #[test]
    fn parses_connect_service() {
        assert_eq!(
            services(&Fake(
                "adb-phone-test _adb-tls-connect._tcp 192.168.18.59:37001\n"
            ))
            .unwrap(),
            vec![Service {
                name: "adb-phone-test".into(),
                kind: Kind::Connect,
                address: "192.168.18.59:37001".into()
            }]
        );
    }
    #[test]
    fn chooses_saved_ip_among_several() {
        assert_eq!(choose(&parse("adb-other-a _adb-tls-connect._tcp 192.168.18.60:30001\nadb-phone-b _adb-tls-connect._tcp 192.168.18.59:37001"), Some("192.168.18.59:5555"), None).unwrap().address, "192.168.18.59:37001");
    }
    #[test]
    fn chooses_saved_device_after_ip_and_suffix_change() {
        assert_eq!(choose(&parse("adb-other-a _adb-tls-connect._tcp 192.168.18.59:30001\nadb-phone-new _adb-tls-connect._tcp 192.168.18.70:37001"), Some("192.168.18.59:5555"), Some("adb-phone-old")).unwrap().address, "192.168.18.70:37001");
    }
    #[test]
    fn qr_payload_has_android_format() {
        assert_eq!(
            payload("studio-0123", "abcd5678"),
            "WIFI:T:ADB;S:studio-0123;P:abcd5678;;"
        );
    }
    #[test]
    fn empty_scan_shows_rescan_and_repair_fallback() {
        let (_tx, rx) = std::sync::mpsc::channel();
        assert_eq!(discover(&Fake("List of discovered mdns services\n"), &rx, 1, |s| choose(s, None, None)), Err("找不到手機的無線偵錯服務。請確認手機與電腦在同一 Wi-Fi，並開啟手機的無線偵錯；再重新掃描或重新配對。".into()));
    }
    #[test]
    fn new_config_shows_choices() {
        assert!(first_run(&crate::scrcpy::Options::default()));
    }
    #[test]
    fn configured_usb_shows_main_screen() {
        assert!(!first_run(&crate::scrcpy::Options {
            mode: Some(crate::preferences::Mode::Usb),
            ..Default::default()
        }));
    }
    #[test]
    fn scan_cancelled_before_start_does_not_run_adb() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(()).unwrap();
        assert_eq!(
            discover(&Fake("must not be read"), &rx, 5, |_| panic!("cancelled")),
            Err("已停止掃描".into())
        );
    }
    #[test]
    fn qr_pair_failure_does_not_expose_adb_output() {
        struct Echo;
        impl CommandRunner for Echo {
            fn run(&self, _: &str, _: &[&str]) -> io::Result<CommandOutput> {
                panic!("timeout required")
            }
            fn run_timeout(
                &self,
                program: &str,
                args: &[&str],
                timeout: Duration,
            ) -> io::Result<TimedOutput> {
                assert_eq!(
                    (program, args, timeout),
                    (
                        "adb",
                        ["pair", "192.168.18.59:44517", "private-test-value"].as_slice(),
                        Duration::from_secs(15)
                    )
                );
                Ok(TimedOutput::Exited(CommandOutput {
                    code: Some(1),
                    stdout: "private-test-value".into(),
                    stderr: "private-test-value".into(),
                }))
            }
        }
        assert_eq!(
            pair_qr(&Echo, "192.168.18.59:44517", "private-test-value"),
            Err("QR 配對失敗，請在手機重新開啟 QR 掃描並重新配對。".into())
        );
    }
    #[test]
    fn configured_wifi_shows_main_screen() {
        assert!(!first_run(&crate::scrcpy::Options {
            mode: Some(crate::preferences::Mode::Wifi),
            wifi_address: Some("192.168.18.59:37001".into()),
            ..Default::default()
        }));
    }
    #[test]
    fn ambiguous_devices_are_not_chosen_arbitrarily() {
        assert_eq!(
            choose(
                &parse(
                    "adb-one-a _adb-tls-connect._tcp 192.168.18.60:30001\nadb-two-b _adb-tls-connect._tcp 192.168.18.59:37001"
                ),
                None,
                None
            ),
            None
        );
    }
}
