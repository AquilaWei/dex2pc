use super::{Action, App, Event, Mode, Quality, Status, egui, input, pair, thread, worker};

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
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("●")
                            .size(26.0)
                            .color(status_color(self.model.status)),
                    );
                    ui.heading(egui::RichText::new(self.model.status.label()).size(28.0));
                });
                self.mode_toggle(ui);
                let running = self.model.session.running || self.busy;
                let label = if running {
                    "停止"
                } else {
                    primary_label(self.model.status)
                };
                if ui
                    .add_enabled(
                        running || (self.environment_done && self.install.is_empty()),
                        egui::Button::new(egui::RichText::new(label).size(20.0))
                            .min_size(egui::vec2(ui.available_width(), 48.0)),
                    )
                    .clicked()
                {
                    self.action(if running { Action::Stop } else { Action::Start });
                }
                self.errors(ui);
                if self.model.options.mode == Some(Mode::Wifi) {
                    ui.add_enabled_ui(!running, |ui| self.wifi(ui));
                }
                self.secondary(ui, ctx);
            });
        });
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

    fn secondary(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
        ui.collapsing("設定", |ui| {
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
