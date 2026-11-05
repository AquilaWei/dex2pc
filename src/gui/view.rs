use super::controller::{self, Screen};
use super::{Action, App, Event, Mode, Quality, Status, egui, input, pair, thread, worker};

fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(20)
        .inner_margin(20)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            contents(ui);
        });
}

fn primary_label(status: Status) -> &'static str {
    match status {
        Status::Connecting | Status::Projecting => "停止",
        Status::Idle | Status::Error => "開啟 DeX",
    }
}

fn status_color(status: Status) -> egui::Color32 {
    match status {
        Status::Idle => egui::Color32::from_rgb(140, 140, 140),
        Status::Connecting => egui::Color32::from_rgb(220, 155, 30),
        Status::Projecting => egui::Color32::from_rgb(40, 170, 100),
        Status::Error => egui::Color32::from_rgb(220, 65, 65),
    }
}

impl App {
    pub(super) fn panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 14.0);
            ui.spacing_mut().button_padding = egui::vec2(16.0, 10.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(notice) = self.font_notice {
                    ui.label(notice);
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("DeX").size(36.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("⚙ 設定").clicked() {
                            self.interface.screen =
                                if matches!(self.interface.screen, Screen::Settings) {
                                    Screen::Main
                                } else {
                                    Screen::Settings
                                };
                        }
                    });
                });
                ui.add_space(12.0);
                match self.interface.screen {
                    Screen::Choices => self.choices(ui),
                    Screen::Main => self.main_screen(ui),
                    Screen::Settings => self.secondary(ui),
                }
                self.install_hints(ui, ctx);
            });
        });
    }

    fn choices(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("讓手機，成為你的桌面。").size(22.0));
        ui.weak("選擇連線方式，完成一次設定後即可一鍵開啟。");
        for (mode, title) in [
            (Mode::Wifi, "無線連線\n掃描 QR 圖碼，輕鬆配對"),
            (Mode::Usb, "USB 連線\n接上傳輸線，允許 USB 偵錯"),
        ] {
            if ui
                .add_enabled(
                    !self.busy && self.install.is_empty(),
                    egui::Button::new(egui::RichText::new(title).size(19.0))
                        .corner_radius(18)
                        .min_size(egui::vec2(ui.available_width(), 100.0)),
                )
                .clicked()
            {
                self.select_setup(mode);
            }
        }
        self.errors(ui);
    }

    fn main_screen(&mut self, ui: &mut egui::Ui) {
        if self.interface.pairing.is_some() {
            self.pairing_screen(ui);
            return;
        }
        card(ui, |ui| {
            let name = if self.interface.device_name.is_empty() {
                "你的 Android 手機"
            } else {
                &self.interface.device_name
            };
            ui.label(egui::RichText::new(name).size(24.0).strong());
            ui.weak(if self.model.options.mode == Some(Mode::Wifi) {
                "Wi-Fi · 無線連線"
            } else {
                "USB · 有線連線"
            });
            ui.horizontal(|ui| {
                ui.colored_label(status_color(self.model.status), "●");
                ui.label(self.model.status.label());
            });
            self.errors(ui);
            if self.interface.fallback {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("重新掃描").clicked() {
                        self.interface.manual = false;
                        self.action(Action::Start);
                    }
                    if ui.button("手動輸入埠號").clicked() {
                        self.interface.manual = true;
                    }
                });
            }
        });
        ui.add_space(12.0);
        let running = self.model.session.running || self.busy;
        let label = if running {
            "停止"
        } else {
            primary_label(self.model.status)
        };
        if ui
            .add_enabled(
                running || (self.environment_done && self.install.is_empty()),
                egui::Button::new(
                    egui::RichText::new(label)
                        .size(21.0)
                        .color(egui::Color32::WHITE),
                )
                .fill(egui::Color32::from_rgb(0, 122, 255))
                .corner_radius(30)
                .min_size(egui::vec2(ui.available_width(), 58.0)),
            )
            .clicked()
        {
            self.action(if running { Action::Stop } else { Action::Start });
        }
        if self.interface.manual && self.model.options.mode == Some(Mode::Wifi) {
            ui.add_enabled_ui(!running, |ui| self.wifi(ui));
        } else {
            ui.weak("設定會自動儲存，下次直接開啟 DeX。");
        }
    }

    fn pairing_screen(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("掃描，開始無線連線");
            ui.label("開發人員選項 → 無線偵錯 → 使用 QR 圖碼配對裝置");
            if let Some(pairing) = &self.interface.pairing {
                controller::draw_qr(ui, pairing);
            }
            ui.weak(if self.busy {
                "等待手機掃描…"
            } else {
                "掃描已結束，請重新產生 QR 圖碼。"
            });
            self.errors(ui);
        });
        if self.busy {
            if ui.button("停止配對").clicked() {
                self.action(Action::Stop);
            }
        } else if ui.button("重新配對").clicked() {
            self.begin_qr();
        }
        if !self.busy && ui.button("改用六位配對碼").clicked() {
            self.interface.pairing = None;
            self.interface.manual = true;
        }
    }

    fn mode_toggle(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (mode, label) in [(Mode::Usb, "USB"), (Mode::Wifi, "Wi-Fi")] {
                if ui
                    .add_enabled(
                        !self.model.session.running && !self.busy,
                        egui::Button::new(label)
                            .selected(self.model.options.mode.unwrap_or_default() == mode)
                            .min_size(egui::vec2(100.0, 38.0)),
                    )
                    .clicked()
                {
                    self.action(Action::Mode(mode));
                }
            }
        });
    }

    fn errors(&self, ui: &mut egui::Ui) {
        if let Some(error) = self.load_error.as_ref().or(self.model.error.as_ref()) {
            egui::Frame::group(ui.style())
                .fill(status_color(Status::Error).gamma_multiply(0.12))
                .stroke(egui::Stroke::new(1.0_f32, status_color(Status::Error)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.colored_label(status_color(Status::Error), error);
                });
        }
    }

    fn wifi(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        let saved = input::host(self.model.options.wifi_address.as_deref()).is_some();
        if saved {
            ui.horizontal(|ui| {
                ui.label("手機 IP");
                if self.wifi_fields.editing {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.wifi_fields.ip)
                            .desired_width(180.0)
                            .hint_text("192.168.18.59"),
                    );
                } else {
                    ui.strong(&self.wifi_fields.ip);
                    if ui.small_button("改 IP").clicked() {
                        self.wifi_fields.editing = true;
                    }
                }
            });
            ui.label("手機「無線偵錯」頁面上的連接埠");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.wifi_fields.port)
                        .desired_width(120.0)
                        .hint_text("連線埠"),
                );
                if ui.button("連線").clicked() {
                    self.connect_phone();
                }
            });
        } else {
            ui.label("手機「無線偵錯」頁面上的 IP 位址和連接埠");
            ui.add(egui::TextEdit::singleline(&mut self.address).hint_text("192.168.18.59:37001"));
            ui.horizontal(|ui| {
                if ui.button("連線").clicked() {
                    self.connect_phone();
                }
                if ui.button("用 USB 自動設定").clicked() {
                    self.action(Action::SwitchWifi);
                }
            });
        }
        ui.collapsing("首次配對", |ui| {
            ui.label("先配對，再連線");
            ui.label("「使用配對碼配對裝置」視窗中的連接埠");
            if !saved {
                ui.small("先在上方填入完整 IP:埠，再填配對埠。");
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.pair_address).hint_text("配對埠，例如 44517"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.code)
                    .password(true)
                    .char_limit(6)
                    .hint_text("六位配對碼"),
            );
            if ui.button("配對").clicked() {
                self.pair_phone();
            }
        });
    }

    fn install_hints(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        for command in &self.install {
            ui.horizontal_wrapped(|ui| {
                ui.label(command);
                if ui.button("複製").clicked() {
                    ctx.copy_text(command.clone());
                }
            });
        }
        if !self.install.is_empty()
            && ui
                .add_enabled(!self.busy, egui::Button::new("重新檢查環境"))
                .clicked()
        {
            self.environment();
        }
    }

    fn secondary(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading("設定");
            self.mode_toggle(ui);
            if ui
                .add_enabled(
                    !self.busy && !self.model.session.running,
                    egui::Button::new("重新配對（QR 圖碼）"),
                )
                .clicked()
            {
                self.model.options.mode = Some(Mode::Wifi);
                self.begin_qr();
            }
            if ui.button("手動輸入埠號").clicked() {
                self.interface.manual = true;
                self.interface.screen = Screen::Main;
            }
            if ui
                .add_enabled(
                    !self.busy && !self.model.session.running,
                    egui::Button::new("切換到 Wi-Fi（先接 USB）"),
                )
                .clicked()
            {
                self.action(Action::SwitchWifi);
            }
            ui.label("畫質（自動儲存）");
            ui.horizontal(|ui| {
                for (quality, label) in [
                    (Quality::High, "高 16M"),
                    (Quality::Balanced, "平衡 8M"),
                    (Quality::Low, "低 4M"),
                ] {
                    if ui
                        .selectable_label(
                            self.model.options.quality.unwrap_or_default() == quality,
                            label,
                        )
                        .clicked()
                    {
                        self.action(Action::Quality(quality));
                    }
                }
            });
            if ui.button("結束 dex2pc").clicked() {
                self.action(Action::Quit);
            }
        });
        ui.collapsing("詳細紀錄", |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.label(&self.model.log);
                });
        });
    }

    fn connect_phone(&mut self) {
        let saved = input::host(self.model.options.wifi_address.as_deref());
        let result = if saved.is_some() {
            input::endpoint(&self.wifi_fields.port, Some(&self.wifi_fields.ip))
        } else {
            input::endpoint(&self.address, None)
        };
        let address = match result {
            Ok(address) => address,
            Err(err) => {
                self.model.fail(err);
                return;
            }
        };
        self.busy = true;
        let path = self.model.path.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let result = crate::wifi::connect_and_save(
                &runner,
                Some(&address),
                None,
                &path,
                &mut thread::sleep,
                &mut worker::Messages::new(tx.clone()),
            )
            .map(|()| address);
            let _ = tx.send(Event::Switched(result.map_err(|err| err.to_string())));
        });
    }

    fn pair_phone(&mut self) {
        let host = if input::host(self.model.options.wifi_address.as_deref()).is_some() {
            Some(self.wifi_fields.ip.as_str())
        } else {
            input::host(Some(&self.address))
        };
        let address = match input::endpoint(&self.pair_address, host) {
            Ok(address) => address,
            Err(err) => {
                self.model.fail(err);
                return;
            }
        };
        self.busy = true;
        let code = std::mem::take(&mut self.code);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let result = pair(&runner, &address, &code);
            let _ = tx.send(Event::Paired(result));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_primary_label() {
        assert_eq!(primary_label(Status::Idle), "開啟 DeX");
    }
    #[test]
    fn connecting_primary_label() {
        assert_eq!(primary_label(Status::Connecting), "停止");
    }
    #[test]
    fn projecting_primary_label() {
        assert_eq!(primary_label(Status::Projecting), "停止");
    }
    #[test]
    fn error_primary_label() {
        assert_eq!(primary_label(Status::Error), "開啟 DeX");
    }
    #[test]
    fn idle_dot_is_grey() {
        assert_eq!(
            status_color(Status::Idle),
            egui::Color32::from_rgb(140, 140, 140)
        );
    }
    #[test]
    fn connecting_dot_is_amber() {
        assert_eq!(
            status_color(Status::Connecting),
            egui::Color32::from_rgb(220, 155, 30)
        );
    }
    #[test]
    fn projecting_dot_is_green() {
        assert_eq!(
            status_color(Status::Projecting),
            egui::Color32::from_rgb(40, 170, 100)
        );
    }
    #[test]
    fn error_dot_is_red() {
        assert_eq!(
            status_color(Status::Error),
            egui::Color32::from_rgb(220, 65, 65)
        );
    }
}
