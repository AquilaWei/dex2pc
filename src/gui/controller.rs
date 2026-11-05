use super::text::{self, Key};
use super::{App, Event, Mode, Quality, Status, device, egui, mpsc, thread, wireless, worker};
use crate::preferences::Language;
use crate::runner::CommandRunner;
use std::path::Path;

#[derive(Default)]
pub(super) enum Screen {
    Choices,
    #[default]
    Main,
    Settings,
    Usb,
}

#[derive(Default)]
pub(super) struct Interface {
    pub screen: Screen,
    pub fallback: bool,
    pub pairing: Option<wireless::Pairing>,
    pub cancel: Option<mpsc::Sender<()>>,
    pub device_name: String,
    pub service_name: String,
    pub preferred: Option<String>,
    pub clear_requested: bool,
    pub back_requested: bool,
}

impl Interface {
    pub fn load(path: &Path, options: &crate::scrcpy::Options) -> std::io::Result<Self> {
        let mut state = Self {
            screen: if wireless::first_run(options) {
                Screen::Choices
            } else {
                Screen::Main
            },
            ..Default::default()
        };
        match std::fs::read_to_string(path.with_file_name("gui-device.toml")) {
            Ok(text) => {
                let table: toml::Table = text.parse().map_err(std::io::Error::other)?;
                state.device_name = table
                    .get("model")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .into();
                state.service_name = table
                    .get("service")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .into();
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (),
            Err(err) => return Err(err),
        }
        Ok(state)
    }
    fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let table = toml::Table::from_iter([
            ("model".into(), self.device_name.clone().into()),
            ("service".into(), self.service_name.clone().into()),
        ]);
        std::fs::write(path.with_file_name("gui-device.toml"), table.to_string())
    }
}

pub(super) enum WirelessEvent {
    Connected(Result<(wireless::Service, String), String>),
    Paired(Result<String, String>),
    Usb(Result<String, String>),
    UsbNotice(Key),
}

impl App {
    pub(super) fn select_setup(&mut self, mode: Mode) {
        self.model.options.mode = Some(mode);
        self.model
            .options
            .quality
            .get_or_insert_with(Quality::default);
        self.interface.screen = Screen::Main;
        if mode == Mode::Wifi {
            self.begin_qr();
        } else {
            self.begin_usb();
        }
    }

    pub(super) fn begin_qr(&mut self) {
        if self.busy || self.model.session.running {
            return;
        }
        let pairing = match wireless::Pairing::new() {
            Ok(pairing) => pairing,
            Err(err) => {
                self.model.fail(format!(
                    "{} {err}",
                    text::get(self.model.language(), Key::QrFailed)
                ));
                return;
            }
        };
        self.interface.screen = Screen::Main;
        self.interface.pairing = Some(pairing);
        self.scan_qr();
    }

    pub(super) fn scan_qr(&mut self) {
        if self.busy || self.model.session.running {
            return;
        }
        let Some(pairing) = &self.interface.pairing else {
            return;
        };
        let name = pairing.name.clone();
        let password = pairing.password();
        self.interface.fallback = false;
        self.model.error = None;
        self.busy = true;
        self.model.session.stopping = false;
        let (cancel, cancelled) = mpsc::channel();
        self.interface.cancel = Some(cancel);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let result = wireless::discover(&runner, &cancelled, 60, |entries| {
                entries
                    .iter()
                    .find(|s| s.kind == wireless::Kind::Pair && s.name == name)
                    .cloned()
            })
            .and_then(|service| {
                if cancelled.try_recv().is_ok() {
                    return Err(text::zh(Key::Cancelled).into());
                }
                wireless::pair_qr(&runner, &service.address, &password)?;
                Ok(service.address)
            });
            let _ = tx.send(Event::Wireless(WirelessEvent::Paired(result)));
        });
    }

    pub(super) fn auto_connect(&mut self) {
        let tx = self.tx.clone();
        let saved = self
            .interface
            .preferred
            .take()
            .or_else(|| self.model.options.wifi_address.clone());
        let device = self.interface.service_name.clone();
        let path = self.model.path.clone();
        let (cancel, cancelled) = mpsc::channel();
        self.interface.cancel = Some(cancel);
        self.interface.fallback = false;
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let result = wireless::discover(&runner, &cancelled, 5, |entries| {
                wireless::choose(entries, saved.as_deref(), Some(&device))
            })
            .and_then(|service| {
                if cancelled.try_recv().is_ok() {
                    return Err(text::zh(Key::Cancelled).into());
                }
                connect_service(
                    &runner,
                    service,
                    &path,
                    &mut worker::Messages::new(tx.clone()),
                )
            });
            let _ = tx.send(Event::Wireless(WirelessEvent::Connected(result)));
        });
    }

    pub(super) fn wireless_event(&mut self, event: WirelessEvent) {
        if let WirelessEvent::UsbNotice(key) = event {
            self.model.error = Some(text::get(self.model.language(), key).into());
            return;
        }
        self.interface.cancel = None;
        self.command.clear();
        self.busy = false;
        if self.model.session.stopping {
            self.model.finish(Status::Idle, None);
            return;
        }
        match event {
            WirelessEvent::Connected(Ok((service, model))) => {
                self.connected(service.address);
                if self.model.error.is_some() {
                    self.model.session.running = false;
                    return;
                }
                self.interface.device_name = model;
                self.interface.service_name = service.name;
                if let Err(err) = self.interface.save(&self.model.path) {
                    self.model.finish(
                        Status::Error,
                        Some(text::message(
                            Key::MetadataFailed,
                            &[("{detail}", &err.to_string())],
                        )),
                    );
                    return;
                }
                self.interface.pairing = None;
                self.model.status = Status::Connecting;
                let _ = self.tx.send(Event::LaunchSession);
            }
            WirelessEvent::Connected(Err(err)) => {
                self.interface.fallback = true;
                self.model.finish(Status::Error, Some(err));
            }
            WirelessEvent::Paired(Ok(address)) => {
                // Keep only the IP as a discovery preference; never save a pairing port as a connect port.
                self.interface.preferred = Some(address);
                self.interface.service_name.clear();
                self.interface.pairing = None;
                self.model.error = None;
                self.model.status = Status::Idle;
                self.model.session.running = true;
                self.model.status = Status::Connecting;
                self.auto_connect();
            }
            WirelessEvent::Paired(Err(err)) => {
                self.interface.fallback = true;
                self.model.fail(err);
            }
            WirelessEvent::Usb(Err(err)) => self.model.fail(err),
            WirelessEvent::Usb(Ok(name)) => {
                self.interface.device_name = name;
                self.interface.service_name.clear();
                if let Err(err) = self.interface.save(&self.model.path) {
                    self.model.fail(format!(
                        "{} {err}",
                        text::get(Language::ZhTw, Key::ConfigFailed)
                    ));
                    return;
                }
                self.model.options.mode = Some(Mode::Usb);
                self.interface.screen = Screen::Main;
                self.action(super::Action::Start);
            }
            WirelessEvent::UsbNotice(_) => unreachable!("handled before terminal events"),
        }
    }
}

pub(super) fn draw_qr(ui: &mut egui::Ui, pairing: &wireless::Pairing) {
    // A white quiet zone is required even in dark mode; draw modules without an image dependency.
    let modules = u16::try_from(pairing.qr.width()).expect("QR width is at most 177 modules");
    let scale = (ui.available_width().min(200.0) / f32::from(modules + 8))
        .floor()
        .max(1.0);
    let size = f32::from(modules + 8) * scale;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 8.0, egui::Color32::WHITE);
    for y in 0..modules {
        for x in 0..modules {
            if pairing.qr[(usize::from(x), usize::from(y))] == qrcode::Color::Dark {
                let origin =
                    rect.min + egui::vec2(f32::from(x + 4) * scale, f32::from(y + 4) * scale);
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(origin, egui::vec2(scale, scale)),
                    0.0,
                    egui::Color32::BLACK,
                );
            }
        }
    }
}

impl App {
    pub(super) fn begin_usb(&mut self) {
        self.interface.screen = Screen::Usb;
        self.model.error = None;
        self.model.session.stopping = false;
        self.busy = true;
        let (cancel, cancelled) = mpsc::channel();
        self.interface.cancel = Some(cancel);
        let tx = self.tx.clone();
        let language = self.model.language();
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let result = wait_usb(&runner, &cancelled, 60, language, |key| {
                let _ = tx.send(Event::Wireless(WirelessEvent::UsbNotice(key)));
            });
            let _ = tx.send(Event::Wireless(WirelessEvent::Usb(result)));
        });
    }
    pub(super) fn back_to_choices(&mut self) {
        self.interface.back_requested = true;
        self.action(super::Action::Stop);
    }
    pub(super) fn clear_connection(&mut self) {
        self.interface.clear_requested = true;
        self.action(super::Action::Stop);
    }
    pub(super) fn deferred_setup_actions(&mut self) {
        // Wait for all prior workers to acknowledge cancellation before rewriting config.
        if self.busy || self.model.session.running {
            return;
        }
        if self.interface.clear_requested {
            self.interface.clear_requested = false;
            // All earlier workers have finished; the new disconnect must not
            // inherit the Stop request that cancelled the old session.
            self.model.session.stopping = false;
            self.busy = true;
            let path = self.model.path.clone();
            let mut options = self.model.options.clone();
            let tx = self.tx.clone();
            let language = self.model.language();
            thread::spawn(move || {
                let runner = worker::SessionRunner::new(tx.clone());
                let result = clear_saved(&runner, &path, &mut options, language);
                let _ = tx.send(Event::Cleared(result));
            });
        } else if self.interface.back_requested {
            self.interface.back_requested = false;
            match crate::config::load(&self.model.path) {
                Ok(options) => self.model.options = options.unwrap_or_default(),
                Err(err) => {
                    self.model.fail(err.to_string());
                    return;
                }
            }
            self.reset_choices();
        }
    }
    fn reset_choices(&mut self) {
        self.interface.screen = Screen::Choices;
        self.interface.pairing = None;
        self.interface.fallback = false;
        self.interface.cancel = None;
        self.model.session.stopping = false;
        self.model.status = Status::Idle;
        self.model.error = None;
        self.model.visible = true;
    }
    pub(super) fn cleared(&mut self, result: Result<(), String>) {
        self.busy = false;
        self.command.clear();
        match result {
            Ok(()) => {
                self.model.options.mode = None;
                self.model.options.wifi_address = None;
                self.interface.device_name.clear();
                self.interface.service_name.clear();
                self.interface.preferred = None;
                self.reset_choices();
            }
            Err(err) => self.model.fail(err),
        }
    }
}

pub(super) fn connect_service(
    runner: &dyn CommandRunner,
    service: wireless::Service,
    path: &Path,
    out: &mut dyn std::io::Write,
) -> Result<(wireless::Service, String), String> {
    crate::wifi::connect_and_save(
        runner,
        Some(&service.address),
        None,
        path,
        &mut thread::sleep,
        out,
    )
    .map_err(|err| {
        format!(
            "{}{}",
            text::message(Key::ConnectFailed, &[("{detail}", &err.to_string())]),
            worker::NETWORK_HINT
        )
    })?;
    let model = runner
        .run("adb", &["devices", "-l"])
        .ok()
        .filter(crate::runner::CommandOutput::success)
        .and_then(|output| {
            device::parse_devices(&output.stdout)
                .into_iter()
                .find(|d| d.serial == service.address || d.serial.starts_with(&service.name))
        })
        .and_then(|d| d.model)
        .unwrap_or_default();
    Ok((service, model))
}

fn clear_saved(
    runner: &dyn CommandRunner,
    path: &Path,
    options: &mut crate::scrcpy::Options,
    language: Language,
) -> Result<(), String> {
    if let Some(address) = &options.wifi_address {
        let output = runner
            .run_timeout(
                "adb",
                &["disconnect", address],
                std::time::Duration::from_secs(2),
            )
            .map_err(|_| text::get(language, Key::DisconnectFailed).to_owned())?;
        if !matches!(output, crate::runner::TimedOutput::Exited(ref output) if output.success()) {
            return Err(text::get(language, Key::DisconnectFailed).into());
        }
    }
    options.mode = None;
    options.wifi_address = None;
    crate::config::clear_gui_connection(path)
        .map_err(|err| format!("{} {err}", text::get(language, Key::ConfigFailed)))
}

fn usb_device(output: &str) -> Result<Option<String>, Key> {
    let devices: Vec<_> = device::parse_devices(output)
        .into_iter()
        .filter(|d| !d.serial.contains(':') && !d.serial.contains("._tcp"))
        .collect();
    if devices.len() > 1 {
        return Err(Key::Multiple);
    }
    let Some(device) = devices.first() else {
        return Ok(None);
    };
    match device.state {
        device::State::Device => Ok(Some(
            device
                .model
                .clone()
                .unwrap_or_else(|| device.serial.clone()),
        )),
        device::State::Unauthorized => Err(Key::Unauthorized),
        _ => Err(Key::UsbPermissions),
    }
}

fn wait_usb(
    runner: &dyn CommandRunner,
    cancel: &mpsc::Receiver<()>,
    attempts: usize,
    language: Language,
    mut notice: impl FnMut(Key),
) -> Result<String, String> {
    // 60 polls, 2-second command deadlines and 1-second waits: bounded to 179 seconds.
    for attempt in 0..attempts {
        if cancel.try_recv().is_ok() {
            return Err(text::get(language, Key::Cancelled).into());
        }
        let output =
            runner.run_timeout("adb", &["devices", "-l"], std::time::Duration::from_secs(2));
        match output {
            Ok(crate::runner::TimedOutput::Exited(output)) if output.success() => {
                match usb_device(&output.stdout) {
                    Ok(Some(model)) => return Ok(model),
                    Ok(None) => (),
                    Err(key) => notice(key),
                }
            }
            _ => notice(Key::UsbPermissions),
        }
        if attempt + 1 < attempts
            && cancel
                .recv_timeout(std::time::Duration::from_secs(1))
                .is_ok()
        {
            return Err(text::get(language, Key::Cancelled).into());
        }
    }
    Err(text::get(language, Key::UsbTimeout).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{CommandOutput, TimedOutput};
    struct Usb(&'static str);
    impl CommandRunner for Usb {
        fn run(&self, _: &str, _: &[&str]) -> std::io::Result<CommandOutput> {
            unreachable!()
        }
        fn run_timeout(
            &self,
            program: &str,
            args: &[&str],
            timeout: std::time::Duration,
        ) -> std::io::Result<TimedOutput> {
            assert_eq!(
                (program, args, timeout),
                (
                    "adb",
                    ["devices", "-l"].as_slice(),
                    std::time::Duration::from_secs(2)
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
    fn one_usb_phone_is_ready_to_start() {
        let (_tx, rx) = mpsc::channel();
        assert_eq!(
            wait_usb(
                &Usb("List of devices attached\nphone device model:SM_S918B\n"),
                &rx,
                1,
                Language::En,
                |_| panic!("unexpected notice")
            ),
            Ok("SM_S918B".into())
        );
    }
    #[test]
    fn unauthorized_usb_phone_reports_allow_debugging() {
        let (_tx, rx) = mpsc::channel();
        let mut message = String::new();
        let _ = wait_usb(&Usb("phone unauthorized\n"), &rx, 1, Language::En, |key| {
            message = text::get(Language::En, key).into();
        });
        assert_eq!(message, "Unlock your phone and allow USB debugging.");
    }
    #[test]
    fn several_usb_phones_require_only_one_connection() {
        assert_eq!(
            usb_device("a device\nb device\n")
                .err()
                .map(|key| text::get(Language::En, key)),
            Some("Multiple devices found. Connect only one USB phone.")
        );
    }
    #[test]
    fn usb_wait_is_bounded_when_no_phone_is_connected() {
        let (_tx, rx) = mpsc::channel();
        assert_eq!(wait_usb(&Usb("List of devices attached\n"), &rx, 1, Language::En, |_| panic!("unexpected notice")), Err("USB detection timed out. Check the cable and USB debugging, then go back and retry.".into()));
    }
    #[test]
    fn usb_wait_can_be_cancelled_before_adb() {
        let (tx, rx) = mpsc::channel();
        tx.send(()).unwrap();
        assert_eq!(
            wait_usb(&Usb("unused"), &rx, 1, Language::En, |_| panic!(
                "unexpected notice"
            )),
            Err("Scan cancelled".into())
        );
    }
    struct Disconnect;
    impl CommandRunner for Disconnect {
        fn run(&self, _: &str, _: &[&str]) -> std::io::Result<CommandOutput> {
            unreachable!()
        }
        fn run_timeout(
            &self,
            program: &str,
            args: &[&str],
            timeout: std::time::Duration,
        ) -> std::io::Result<TimedOutput> {
            assert_eq!(
                (program, args, timeout),
                (
                    "adb",
                    ["disconnect", "192.168.18.59:37001"].as_slice(),
                    std::time::Duration::from_secs(2)
                )
            );
            Ok(TimedOutput::Exited(CommandOutput {
                code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            }))
        }
    }
    #[test]
    fn clearing_disconnects_saved_address_and_removes_only_connection_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "mode='wifi'\nwifi_address='192.168.18.59:37001'\nquality='low'\nlanguage='en'\naudio=true\n").unwrap();
        let mut options = crate::config::load(&path).unwrap().unwrap();
        clear_saved(&Disconnect, &path, &mut options, Language::En).unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "audio = true\nlanguage = \"en\"\nquality = \"low\"\n"
        );
    }
    #[test]
    fn clearing_usb_without_address_never_runs_adb() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut options = crate::scrcpy::Options {
            mode: Some(Mode::Usb),
            ..Default::default()
        };
        clear_saved(&Usb("unused"), &path, &mut options, Language::En).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "");
    }
}
