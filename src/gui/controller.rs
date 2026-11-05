use super::{App, Event, Mode, Quality, Status, device, egui, mpsc, thread, wireless, worker};
use crate::runner::CommandRunner;
use std::path::Path;

#[derive(Default)]
pub(super) enum Screen {
    Choices,
    #[default]
    Main,
    Settings,
}

#[derive(Default)]
pub(super) struct Interface {
    pub screen: Screen,
    pub manual: bool,
    pub fallback: bool,
    pub pairing: Option<wireless::Pairing>,
    pub cancel: Option<mpsc::Sender<()>>,
    pub device_name: String,
    pub service_name: String,
    pub preferred: Option<String>,
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
}

impl App {
    pub(super) fn select_setup(&mut self, mode: Mode) {
        self.model.options.mode = Some(mode);
        self.model.options.quality = Some(Quality::default());
        self.interface.screen = Screen::Main;
        if mode == Mode::Wifi {
            self.begin_qr();
        } else {
            self.model.save();
        }
    }

    pub(super) fn begin_qr(&mut self) {
        if self.busy || self.model.session.running {
            return;
        }
        let pairing = match wireless::Pairing::new() {
            Ok(pairing) => pairing,
            Err(err) => {
                self.model
                    .fail(format!("無法建立 QR 配對：{err}；請改用六位配對碼。"));
                return;
            }
        };
        self.interface.screen = Screen::Main;
        let name = pairing.name.clone();
        let password = pairing.password();
        self.interface.pairing = Some(pairing);
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
                    return Err("已停止配對".into());
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
                    return Err("已停止連線".into());
                }
                crate::wifi::connect_and_save(
                    &runner,
                    Some(&service.address),
                    None,
                    &path,
                    &mut thread::sleep,
                    &mut worker::Messages::new(tx.clone()),
                )
                .map_err(|err| format!("無線連線失敗：{err}；{}", worker::NETWORK_HINT))?;
                let model = runner
                    .run("adb", &["devices", "-l"])
                    .ok()
                    .filter(crate::runner::CommandOutput::success)
                    .and_then(|output| {
                        device::parse_devices(&output.stdout).into_iter().find(|d| {
                            d.serial == service.address || d.serial.starts_with(&service.name)
                        })
                    })
                    .and_then(|d| d.model)
                    .unwrap_or_else(|| "Android 手機".into());
                Ok((service, model))
            });
            let _ = tx.send(Event::Wireless(WirelessEvent::Connected(result)));
        });
    }

    pub(super) fn wireless_event(&mut self, event: WirelessEvent) {
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
                        Some(format!("無法儲存手機資訊：{err}；請檢查設定目錄權限。")),
                    );
                    return;
                }
                self.interface.pairing = None;
                self.model.status = Status::Connecting;
                worker::session(self.tx.clone(), self.model.options.clone());
            }
            WirelessEvent::Connected(Err(err)) => {
                self.interface.fallback = true;
                self.model.finish(Status::Error, Some(err));
            }
            WirelessEvent::Paired(Ok(address)) => {
                // Keep only the IP as a discovery preference; never save a pairing port as a connect port.
                self.address = address;
                self.interface.preferred = Some(self.address.clone());
                self.interface.service_name.clear();
                self.interface.pairing = None;
                self.model.error = None;
                self.model.status = Status::Idle;
                self.interface.manual = false;
                self.model.session.running = true;
                self.model.status = Status::Connecting;
                // The connect service normally appears on the same IP as the pairing service.
                self.wifi_fields.ip = super::input::host(Some(&self.address))
                    .unwrap_or_default()
                    .into();
                self.auto_connect();
            }
            WirelessEvent::Paired(Err(err)) => self.model.fail(err),
        }
    }
}

pub(super) fn draw_qr(ui: &mut egui::Ui, pairing: &wireless::Pairing) {
    // A white quiet zone is required even in dark mode; draw modules without an image dependency.
    let modules = u16::try_from(pairing.qr.width()).expect("QR width is at most 177 modules");
    let scale = (ui.available_width().min(240.0) / f32::from(modules + 8))
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
