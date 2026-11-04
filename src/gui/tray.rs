use super::{Action, Event, Status};
use crate::preferences::{Mode, Quality};
use ksni::menu::{RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{MenuItem, ToolTip};
use std::sync::mpsc::Sender;

pub struct Tray {
    pub events: Sender<Event>,
    pub mode: Mode,
    pub quality: Quality,
    pub status: Status,
    pub icons: String,
}
impl Tray {
    fn send(&self, action: Action) {
        let _ = self.events.send(Event::Action(action));
    }
}
fn item(label: &str, action: Action) -> MenuItem<Tray> {
    StandardItem {
        label: label.into(),
        activate: Box::new(move |tray: &mut Tray| tray.send(action)),
        ..Default::default()
    }
    .into()
}
impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "dex2pc".into()
    }
    fn title(&self) -> String {
        format!("dex2pc：{}", self.status.label())
    }
    fn icon_name(&self) -> String {
        format!("dex2pc-{}", self.status.icon())
    }
    fn icon_theme_path(&self) -> String {
        self.icons.clone()
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.title(),
            icon_name: self.icon_name(),
            description: "Samsung DeX 視窗投影".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _: i32, _: i32) {
        self.send(Action::Show);
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        let _ = self.events.send(Event::TrayLost);
        false
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            item("開啟 DeX", Action::Start),
            item("停止 DeX", Action::Stop),
            SubMenu {
                label: "連線方式".into(),
                submenu: vec![
                    RadioGroup {
                        selected: usize::from(self.mode == Mode::Wifi),
                        options: vec![
                            RadioItem {
                                label: "USB".into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "Wi-Fi".into(),
                                ..Default::default()
                            },
                        ],
                        select: Box::new(|tray: &mut Self, index| {
                            tray.send(Action::Mode(if index == 0 {
                                Mode::Usb
                            } else {
                                Mode::Wifi
                            }));
                        }),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "畫質".into(),
                submenu: vec![
                    RadioGroup {
                        selected: match self.quality {
                            Quality::High => 0,
                            Quality::Balanced => 1,
                            Quality::Low => 2,
                        },
                        options: vec![
                            RadioItem {
                                label: "高（16M）".into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "平衡（8M）".into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "低（4M）".into(),
                                ..Default::default()
                            },
                        ],
                        select: Box::new(|tray: &mut Self, index| {
                            tray.send(Action::Quality(match index {
                                0 => Quality::High,
                                1 => Quality::Balanced,
                                _ => Quality::Low,
                            }));
                        }),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            item("切換到 Wi-Fi", Action::SwitchWifi),
            item("開啟設定視窗…", Action::Show),
            item("結束", Action::Quit),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;
    use std::sync::mpsc;
    #[test]
    fn fake_host_start_menu_sends_start_action() {
        let (events, rx) = mpsc::channel();
        let mut tray = Tray {
            events,
            mode: Mode::Usb,
            quality: Quality::Balanced,
            status: Status::Idle,
            icons: String::new(),
        };
        let MenuItem::Standard(item) = tray.menu().remove(0) else {
            panic!("缺少開啟動作");
        };
        (item.activate)(&mut tray);
        assert!(matches!(rx.recv().unwrap(), Event::Action(Action::Start)));
    }
    #[test]
    fn error_tooltip_shows_error_state() {
        let (events, _) = mpsc::channel();
        let tray = Tray {
            events,
            mode: Mode::Usb,
            quality: Quality::Balanced,
            status: Status::Error,
            icons: String::new(),
        };
        assert_eq!(tray.tool_tip().title, "dex2pc：錯誤");
        assert_eq!(tray.icon_name(), "dex2pc-error");
    }
}
