use super::{Action, App, Event, Mode, Quality, egui, io, pair, thread, worker};

impl App {
    pub(super) fn panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(notice) = self.font_notice {
                ui.label(notice);
            }
            ui.heading("DeX 設定與投影");
            ui.label(format!("狀態：{}", self.model.status.label()));
            for command in &self.install {
                ui.horizontal(|ui| {
                    ui.label(command);
                    if ui.button("複製").clicked() {
                        ctx.copy_text(command.clone());
                    }
                });
            }
            if ui
                .add_enabled(
                    !self.busy && !self.model.session.running,
                    egui::Button::new("重新檢查環境"),
                )
                .clicked()
            {
                self.environment();
            }
            if let Some(error) = &self.load_error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            ui.label("1. 檢查 adb / scrcpy；手機需開啟 USB 偵錯並允許授權");
            self.connection(ui);
            ui.label("3. 畫質");
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
            if ui.button("儲存設定").clicked() {
                self.model.save();
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.busy
                            && !self.model.session.running
                            && self.environment_done
                            && self.install.is_empty(),
                        egui::Button::new("開啟 DeX"),
                    )
                    .clicked()
                {
                    self.action(Action::Start);
                }
                if ui
                    .add_enabled(
                        self.model.session.running || self.busy,
                        egui::Button::new("停止"),
                    )
                    .clicked()
                {
                    self.action(Action::Stop);
                }
                if ui.button("結束").clicked() {
                    self.action(Action::Quit);
                }
            });
            if let Some(error) = &self.model.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.label(&self.model.log);
                });
        });
    }
}

impl App {
    fn advanced(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("進階：手動位址與配對", |ui| {
            ui.label("連線 IP:埠");
            ui.text_edit_singleline(&mut self.address);
            if ui
                .add_enabled(
                    !self.busy && !self.model.session.running,
                    egui::Button::new("連線並儲存"),
                )
                .clicked()
            {
                self.busy = true;
                let address = self.address.clone();
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
                        &mut io::sink(),
                    )
                    .map(|()| address);
                    let _ = tx.send(Event::Switched(result.map_err(|err| err.to_string())));
                });
            }
            ui.label("配對 IP:埠（與連線埠不同）");
            ui.text_edit_singleline(&mut self.pair_address);
            ui.add(
                egui::TextEdit::singleline(&mut self.code)
                    .password(true)
                    .hint_text("配對碼"),
            );
            if ui
                .add_enabled(
                    !self.busy && !self.model.session.running,
                    egui::Button::new("配對"),
                )
                .clicked()
            {
                self.busy = true;
                let address = self.pair_address.clone();
                let code = std::mem::take(&mut self.code);
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let runner = worker::SessionRunner::new(tx.clone());
                    let result = pair(&runner, &address, &code);
                    let _ = tx.send(Event::Paired(result));
                });
            }
        });
    }
    fn connection(&mut self, ui: &mut egui::Ui) {
        ui.label("2. 選擇連線方式");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    self.model.options.mode.unwrap_or_default() == Mode::Usb,
                    "USB",
                )
                .clicked()
            {
                self.action(Action::Mode(Mode::Usb));
            }
            if ui
                .selectable_label(self.model.options.mode == Some(Mode::Wifi), "Wi-Fi")
                .clicked()
            {
                self.action(Action::Mode(Mode::Wifi));
            }
        });
        if self.model.options.mode == Some(Mode::Wifi) {
            if ui
                .add_enabled(
                    !self.busy && !self.model.session.running,
                    egui::Button::new("切換到 Wi-Fi（先接 USB）"),
                )
                .clicked()
            {
                self.action(Action::SwitchWifi);
            }
            self.advanced(ui);
        } else {
            ui.label(if self.checked {
                "手機已授權"
            } else {
                "等待手機連線／授權，每兩秒重新偵測"
            });
        }
    }
}
