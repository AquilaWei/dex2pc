use super::text::{self, Key};
use super::{Action, Event, Status};
use crate::preferences::{Language, Quality};
use ksni::menu::{RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{MenuItem, ToolTip};
use std::sync::mpsc::Sender;

pub struct Tray {
    pub events: Sender<Event>,
    pub quality: Quality,
    pub language: Language,
    pub running: bool,
    pub status: Status,
    pub icons: String,
}
impl Tray {
    fn send(&self, action: Action) {
        let _ = self.events.send(Event::Action(action));
    }
    fn t(&self, key: Key) -> &'static str {
        text::get(self.language, key)
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
        format!("dex2pc: {}", text::status(self.language, self.status))
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
            description: self.t(Key::Tooltip).into(),
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
            item(
                self.t(if self.running { Key::Stop } else { Key::Start }),
                if self.running {
                    Action::Stop
                } else {
                    Action::Start
                },
            ),
            SubMenu {
                label: self.t(Key::Quality).into(),
                submenu: vec![
                    RadioGroup {
                        selected: match self.quality {
                            Quality::High => 0,
                            Quality::Balanced => 1,
                            Quality::Low => 2,
                        },
                        options: [Key::High, Key::Balanced, Key::Low]
                            .map(|key| RadioItem {
                                label: self.t(key).into(),
                                ..Default::default()
                            })
                            .into(),
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
            SubMenu {
                label: self.t(Key::Language).into(),
                submenu: vec![
                    RadioGroup {
                        selected: usize::from(self.language == Language::En),
                        options: [Key::Chinese, Key::English]
                            .map(|key| RadioItem {
                                label: self.t(key).into(),
                                ..Default::default()
                            })
                            .into(),
                        select: Box::new(|tray: &mut Self, index| {
                            tray.send(Action::Language(if index == 0 {
                                Language::ZhTw
                            } else {
                                Language::En
                            }));
                        }),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            item(self.t(Key::Clear), Action::Clear),
            item(self.t(Key::Show), Action::Show),
            item(self.t(Key::Quit), Action::Quit),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;
    fn menu(language: Language) -> Vec<String> {
        let (events, _) = std::sync::mpsc::channel();
        Tray {
            events,
            quality: Quality::Balanced,
            language,
            running: false,
            status: Status::Idle,
            icons: String::new(),
        }
        .menu()
        .into_iter()
        .map(|item| match item {
            MenuItem::Standard(item) => item.label,
            MenuItem::SubMenu(item) => item.label,
            _ => panic!("unexpected menu item"),
        })
        .collect()
    }
    #[test]
    fn chinese_menu_order() {
        assert_eq!(
            menu(Language::ZhTw),
            [
                "開啟 DeX",
                "畫質",
                "語言",
                "清除連線設定",
                "顯示視窗",
                "結束"
            ]
        );
    }
    #[test]
    fn english_menu_order() {
        assert_eq!(
            menu(Language::En),
            [
                "Start DeX",
                "Quality",
                "Language",
                "Clear connection settings",
                "Show window",
                "Quit"
            ]
        );
    }
}
