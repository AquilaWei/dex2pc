use super::controller::{self, Screen};
use super::text::{self, Key};
use super::{Action, App, Mode, Quality, Status, egui};
use crate::preferences::Language;

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
fn status_color(status: Status) -> egui::Color32 {
    match status {
        Status::Idle => egui::Color32::from_rgb(140, 140, 140),
        Status::Connecting => egui::Color32::from_rgb(220, 155, 30),
        Status::Projecting => egui::Color32::from_rgb(40, 170, 100),
        Status::Error => egui::Color32::from_rgb(220, 65, 65),
    }
}
pub(super) fn fallback_actions() -> [Key; 2] {
    [Key::Rescan, Key::Repair]
}
impl App {
    fn t(&self, key: Key) -> &'static str {
        text::get(self.model.language(), key)
    }
    pub(super) fn panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 14.0);
            ui.spacing_mut().button_padding = egui::vec2(16.0, 10.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.font_notice.is_some() {
                    ui.label(super::fonts::NOTICE);
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new("DeX").size(36.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(format!("⚙ {}", self.t(Key::Settings))).clicked() {
                            self.interface.screen =
                                if matches!(self.interface.screen, Screen::Settings) {
                                    if super::wireless::first_run(&self.model.options) {
                                        Screen::Choices
                                    } else {
                                        Screen::Main
                                    }
                                } else {
                                    Screen::Settings
                                };
                        }
                    });
                });
                ui.add_space(12.0);
                match self.interface.screen {
                    Screen::Choices => self.choices(ui),
                    Screen::Usb => self.usb_screen(ui),
                    Screen::Main => self.main_screen(ui),
                    Screen::Settings => self.secondary(ui),
                }
                self.install_hints(ui, ctx);
            });
        });
    }
    fn choices(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(self.t(Key::Title)).size(22.0));
        ui.weak(self.t(Key::Choose));
        for (mode, key) in [(Mode::Usb, Key::Usb), (Mode::Wifi, Key::Wifi)] {
            if ui
                .add_enabled(
                    !self.busy && self.environment_done && self.install.is_empty(),
                    egui::Button::new(egui::RichText::new(self.t(key)).size(22.0))
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
    fn back_button(&mut self, ui: &mut egui::Ui) {
        if ui.button(self.t(Key::Back)).clicked() {
            self.back_to_choices();
        }
    }
    fn usb_screen(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading(self.t(Key::Usb));
            ui.label(self.t(Key::UsbHint));
            if self.busy {
                ui.spinner();
                ui.weak(self.t(Key::Waiting));
            }
            self.errors(ui);
        });
        self.back_button(ui);
    }
    fn main_screen(&mut self, ui: &mut egui::Ui) {
        if self.interface.pairing.is_some() {
            self.pairing_screen(ui);
            return;
        }
        card(ui, |ui| {
            let name = if self.interface.device_name.is_empty() {
                self.t(Key::Phone)
            } else {
                &self.interface.device_name
            };
            ui.label(egui::RichText::new(name).size(24.0).strong());
            ui.weak(self.t(if self.model.options.mode == Some(Mode::Wifi) {
                Key::Wifi
            } else {
                Key::Usb
            }));
            ui.horizontal(|ui| {
                ui.colored_label(status_color(self.model.status), "●");
                ui.label(text::status(self.model.language(), self.model.status));
            });
            self.errors(ui);
            if self.interface.fallback {
                ui.horizontal_wrapped(|ui| {
                    for key in fallback_actions() {
                        if ui.button(self.t(key)).clicked() {
                            match key {
                                Key::Rescan => self.action(Action::Start),
                                _ => self.begin_qr(),
                            }
                        }
                    }
                });
            }
        });
        ui.add_space(12.0);
        let running = self.model.session.running || self.interface.cancel.is_some();
        if ui
            .add_enabled(
                running || (!self.busy && self.environment_done && self.install.is_empty()),
                egui::Button::new(
                    egui::RichText::new(self.t(if running { Key::Stop } else { Key::Start }))
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
    }
    fn pairing_screen(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading(self.t(Key::QrTitle));
            ui.label(self.t(Key::WifiHint));
            ui.label(self.t(Key::QrPath));
            if let Some(pairing) = &self.interface.pairing {
                controller::draw_qr(ui, pairing);
            }
            ui.weak(self.t(if self.busy {
                Key::Waiting
            } else {
                Key::QrEnded
            }));
            self.errors(ui);
            if !self.busy {
                ui.horizontal_wrapped(|ui| {
                    for key in fallback_actions() {
                        if ui.button(self.t(key)).clicked() {
                            match key {
                                Key::Rescan => self.scan_qr(),
                                _ => self.begin_qr(),
                            }
                        }
                    }
                });
            }
        });
        self.back_button(ui);
    }
    fn errors(&self, ui: &mut egui::Ui) {
        if let Some(error) = self.load_error.as_ref().or(self.model.error.as_ref()) {
            egui::Frame::group(ui.style())
                .fill(status_color(Status::Error).gamma_multiply(0.12))
                .stroke(egui::Stroke::new(1.0_f32, status_color(Status::Error)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.colored_label(
                        status_color(Status::Error),
                        super::text::diagnostic(self.model.language(), error),
                    );
                });
        }
    }
    fn install_hints(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        for command in &self.install {
            ui.horizontal_wrapped(|ui| {
                ui.label(command);
                if ui.button(self.t(Key::Copy)).clicked() {
                    ctx.copy_text(command.clone());
                }
            });
        }
        if !self.install.is_empty()
            && ui
                .add_enabled(!self.busy, egui::Button::new(self.t(Key::Check)))
                .clicked()
        {
            self.environment();
        }
    }
    fn secondary(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.heading(self.t(Key::Settings));
            ui.label(self.t(Key::Quality));
            ui.horizontal(|ui| {
                for (quality, key) in [
                    (Quality::High, Key::High),
                    (Quality::Balanced, Key::Balanced),
                    (Quality::Low, Key::Low),
                ] {
                    if ui
                        .selectable_label(
                            self.model.options.quality.unwrap_or_default() == quality,
                            self.t(key),
                        )
                        .clicked()
                    {
                        self.action(Action::Quality(quality));
                    }
                }
            });
            ui.label(self.t(Key::Language));
            ui.horizontal(|ui| {
                for (language, key) in
                    [(Language::ZhTw, Key::Chinese), (Language::En, Key::English)]
                {
                    if ui
                        .selectable_label(self.model.language() == language, self.t(key))
                        .clicked()
                    {
                        self.action(Action::Language(language));
                    }
                }
            });
            if ui.button(self.t(Key::Clear)).clicked() {
                self.action(Action::Clear);
            }
            self.errors(ui);
        });
        ui.collapsing(self.t(Key::Log), |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.label(&self.model.log);
                });
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn not_found_offers_only_rescan_and_repair() {
        assert_eq!(
            fallback_actions().map(|key| text::get(Language::En, key)),
            ["Rescan", "Pair again"]
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
