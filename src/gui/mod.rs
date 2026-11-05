mod controller;
mod fonts;
mod input;
mod ipc;
mod text;
mod tray;
mod view;
mod wireless;
mod worker;

use crate::preferences::{Language, Mode, Quality};
use crate::runner::CommandRunner;
use crate::{config, device, scrcpy::Options};
use eframe::egui;
use ksni::blocking::TrayMethods;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Idle,
    Connecting,
    Projecting,
    Error,
}
impl Status {
    fn icon(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Projecting => "projecting",
            Self::Error => "error",
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Start,
    Stop,
    Quality(Quality),
    Language(Language),
    Clear,
    Show,
    Quit,
}
enum Event {
    Action(Action),
    Command(worker::StopHandle, bool),
    Log(String),
    Finished(Status, Option<String>),
    Checked(Result<String, String>),
    TrayLost,
    Environment(Vec<String>),
    Wireless(controller::WirelessEvent),
    Cleared(Result<(), String>),
    LaunchSession,
}

/// UI 的狀態機只處理資料與動作；背景工作、工具列與 IPC 都以 channel 送事件。
#[derive(Default)]
struct Lifecycle {
    running: bool,
    stopping: bool,
    quitting: bool,
}
struct Model {
    options: Options,
    path: PathBuf,
    status: Status,
    session: Lifecycle,
    visible: bool,
    tray: bool,
    error: Option<String>,
    log: String,
}
impl Model {
    fn new(path: PathBuf, loaded: Result<Option<Options>, String>) -> Self {
        let (options, error) = match loaded {
            Ok(options) => (options.unwrap_or_default(), None),
            Err(err) => (Options::default(), Some(err)),
        };
        Self {
            options,
            path,
            status: Status::Idle,
            session: Lifecycle::default(),
            visible: true,
            tray: false,
            error,
            log: String::new(),
        }
    }
    fn close_window(&mut self) {
        if self.tray {
            self.visible = false;
        } else {
            self.choose(Action::Quit);
        }
    }
    fn language(&self) -> Language {
        self.options.language.unwrap_or_else(|| {
            Language::from_locale(
                std::env::var("LC_ALL").ok().as_deref(),
                std::env::var("LANG").ok().as_deref(),
            )
        })
    }
    fn fail(&mut self, message: String) {
        self.error = Some(if message.trim().is_empty() {
            text::get(Language::ZhTw, text::Key::OperationFailed).into()
        } else {
            message
        });
        self.status = Status::Error;
        // Tray-triggered failures must expose their explanation in the window.
        self.visible = true;
    }
    fn finish(&mut self, status: Status, error: Option<String>) {
        self.session.running = false;
        if self.session.stopping {
            self.status = Status::Idle;
            self.error = None;
        } else if status == Status::Error {
            self.fail(error.unwrap_or_default());
        } else {
            self.status = status;
            self.error = error;
        }
    }
    fn save(&mut self) -> bool {
        match config::save_gui(&self.path, &self.options) {
            Ok(()) => true,
            Err(err) => {
                self.fail(text::message(
                    text::Key::SaveFailed,
                    &[
                        ("{path}", &self.path.display().to_string()),
                        ("{detail}", &err.to_string()),
                    ],
                ));
                false
            }
        }
    }
    fn choose(&mut self, action: Action) {
        match action {
            Action::Quality(quality) => {
                self.options.quality = Some(quality);
                self.save();
            }
            Action::Language(language) => {
                self.options.language = Some(language);
                self.save();
            }
            Action::Show => self.visible = true,
            Action::Stop => self.session.stopping = true,
            Action::Quit => {
                self.session.stopping = true;
                self.session.quitting = true;
            }
            Action::Start | Action::Clear => {}
        }
    }
}

fn visibility_command(visible: bool, last: &mut Option<bool>) -> Option<bool> {
    // Avoid asking the compositor to remap an unchanged window on every repaint.
    if *last == Some(visible) {
        return None;
    }
    *last = Some(visible);
    Some(visible)
}

struct App {
    last_visibility: Option<bool>,
    model: Model,
    events: Receiver<Event>,
    tx: Sender<Event>,
    command: Vec<worker::StopHandle>,
    tray: Option<ksni::blocking::Handle<tray::Tray>>,
    _instance: ipc::Instance,
    busy: bool,
    checked: bool,
    next_check: Instant,
    install: Vec<String>,
    environment_done: bool,
    load_error: Option<String>,
    font_notice: Option<&'static str>,
    interface: controller::Interface,
}
impl App {
    fn action(&mut self, action: Action) {
        self.model.choose(action);
        match action {
            Action::Start if !self.model.session.running && !self.busy => {
                if wireless::first_run(&self.model.options) {
                    self.interface.screen = controller::Screen::Choices;
                    self.model.visible = true;
                    return;
                }
                if !self.environment_done || !self.install.is_empty() {
                    self.model
                        .fail(text::get(Language::ZhTw, text::Key::NotReady).into());
                    return;
                }
                if self.load_error.is_some() {
                    self.model
                        .fail(text::zh(text::Key::ConfigUnreadable).into());
                    return;
                }
                if !self.model.save() {
                    return;
                }
                self.model.error = None;
                self.model.log.clear();
                self.model.session.running = true;
                self.model.session.stopping = false;
                self.model.status = Status::Connecting;
                if self.model.options.mode == Some(Mode::Wifi) {
                    self.auto_connect();
                } else {
                    let _ = self.tx.send(Event::LaunchSession);
                }
            }
            Action::Stop | Action::Quit => {
                if let Some(cancel) = &self.interface.cancel {
                    let _ = cancel.send(());
                }
                for tx in &self.command {
                    let _ = tx.send(());
                }
            }
            Action::Clear => self.clear_connection(),
            _ => {}
        }
    }
    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Action(action) => self.action(action),
                Event::Command(tx, projection) => {
                    if self.model.session.stopping {
                        let _ = tx.send(());
                    }
                    self.command.retain(|handle| !handle.finished());
                    self.command.push(tx);
                    if projection {
                        self.model.status = Status::Projecting;
                        self.model.visible = !self.model.tray;
                    }
                }
                Event::Log(text) => {
                    if text.contains("裝置已斷線") || text.contains("連線無回應") {
                        self.model.status = Status::Connecting;
                    }
                    self.model.log.push_str(&text);
                    if self.model.log.len() > 32_768 {
                        self.model.log.clear();
                    }
                }
                Event::Finished(status, error) => {
                    self.model.finish(status, error);
                    self.command.clear();
                }
                Event::Checked(result) => {
                    self.busy = false;
                    if !self.model.session.quitting {
                        self.model.session.stopping = false;
                    }
                    self.checked = result.is_ok();
                    if let Ok(name) = &result {
                        self.interface.device_name.clone_from(name);
                    }
                    if let Err(err) = result {
                        self.model.fail(err);
                    }
                    self.command.clear();
                }
                Event::Environment(commands) => {
                    if !commands.is_empty() {
                        self.model.fail(text::message(
                            text::Key::MissingTools,
                            &[("{detail}", &commands.join("\n"))],
                        ));
                    }
                    self.install = commands;
                    self.environment_done = true;
                    self.busy = false;
                }
                Event::Wireless(event) => self.wireless_event(event),
                Event::Cleared(result) => self.cleared(result),
                Event::LaunchSession => {
                    if self.model.session.stopping {
                        self.model.finish(Status::Idle, None);
                    } else {
                        worker::session(self.tx.clone(), self.model.options.clone());
                    }
                }
                Event::TrayLost => {
                    self.model.tray = false;
                    self.tray = None;
                    self.model.visible = true;
                }
            }
        }
        self.deferred_setup_actions();
        self.update_tray();
        if let Some(visible) = visibility_command(self.model.visible, &mut self.last_visibility) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
        }
        if self.model.session.quitting && !self.model.session.running && !self.busy {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn update_tray(&self) {
        if let Some(handle) = &self.tray {
            let quality = self.model.options.quality.unwrap_or_default();
            let status = self.model.status;
            let language = self.model.language();
            let running = self.model.session.running || self.interface.cancel.is_some();
            handle.update(|tray| {
                tray.quality = quality;
                tray.status = status;
                tray.language = language;
                tray.running = running;
            });
        }
    }
    fn connected(&mut self, address: String) {
        self.model.options.wifi_address = Some(address);
        self.model.options.mode = Some(Mode::Wifi);
        self.model
            .options
            .quality
            .get_or_insert_with(Quality::default);
        self.model.status = Status::Idle;
        self.model.error = None;
        self.model.save();
    }
    fn environment(&mut self) {
        self.busy = true;
        self.environment_done = false;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let commands = install_hints(
                &runner,
                &std::fs::read_to_string("/etc/os-release").unwrap_or_default(),
            );
            let _ = tx.send(Event::Environment(commands));
        });
    }
    fn check(&mut self) {
        self.busy = true;
        self.next_check = Instant::now() + Duration::from_secs(2);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let runner = worker::SessionRunner::new(tx.clone());
            let result = runner.run("adb", &["devices", "-l"]).and_then(|output| {
                if !output.success() {
                    return Err(io::Error::other(output.stderr));
                }
                let devices: Vec<_> = device::parse_devices(&output.stdout)
                    .into_iter()
                    .filter(|d| !d.serial.contains(':'))
                    .collect();
                device::select(&devices, None)
                    .map(|device| {
                        device
                            .model
                            .clone()
                            .unwrap_or_else(|| device.serial.clone())
                    })
                    .map_err(io::Error::other)
            });
            let _ = tx.send(Event::Checked(result.map_err(|err| err.to_string())));
        });
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.pump(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));
        if ctx.input(|input| input.viewport().close_requested()) && !self.model.session.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.model.close_window();
            if self.model.session.quitting {
                self.action(Action::Quit);
            }
        }
        if self.model.visible
            && matches!(self.interface.screen, controller::Screen::Main)
            && !self.model.session.running
            && !self.busy
            && self.environment_done
            && self.load_error.is_none()
            && self.install.is_empty()
            && self.model.options.mode.unwrap_or_default() == Mode::Usb
            && Instant::now() >= self.next_check
        {
            self.check();
        }
        self.panel(ctx);
    }
}

fn install_hints(runner: &dyn CommandRunner, os: &str) -> Vec<String> {
    let fedora = os.contains("fedora");
    let mut commands = Vec::new();
    if !runner
        .run("adb", &["version"])
        .is_ok_and(|output| output.success())
    {
        commands.push(
            if fedora {
                crate::doctor::ADB_INSTALL_HINT
            } else {
                "sudo apt install adb"
            }
            .into(),
        );
    }
    if !runner.run("scrcpy", &["--version"]).is_ok_and(|output| {
        output.success()
            && crate::doctor::parse_scrcpy_version(&output.stdout)
                .is_some_and(|version| version >= crate::doctor::MIN_SCRCPY)
    }) {
        commands.push(
            if fedora {
                crate::doctor::SCRCPY_INSTALL_HINT
            } else {
                "sudo apt install scrcpy"
            }
            .into(),
        );
    }
    commands
}

/// GUI 入口保留 CLI 的 help/version 不依賴顯示伺服器；圖形與 IPC 初始化失敗時回傳錯誤。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = config::path(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            path.parent()
                .unwrap_or(std::path::Path::new("."))
                .join("runtime")
        });
    let dir = runtime.join("dex2pc");
    let (tx, events) = mpsc::channel();
    let Some(instance) = ipc::acquire(&dir, tx.clone())? else {
        return Ok(());
    };
    let model = Model::new(
        path.clone(),
        config::load(&path).map_err(|err| err.to_string()),
    );
    let icons = icons(&dir)?;
    let tray = tray::Tray {
        events: tx.clone(),
        language: model.language(),
        running: false,
        quality: model.options.quality.unwrap_or_default(),
        status: model.status,
        icons: icons.to_string_lossy().into_owned(),
    }
    .spawn()
    .ok();
    let interface = controller::Interface::load(&model.path, &model.options)?;
    let mut app = App {
        last_visibility: None,
        model,
        events,
        tx,
        command: Vec::new(),
        tray,
        _instance: instance,
        busy: true,
        checked: false,
        next_check: Instant::now(),
        install: Vec::new(),
        environment_done: false,
        load_error: None,
        font_notice: None,
        interface,
    };
    app.load_error.clone_from(&app.model.error);
    app.environment();
    app.model.tray = app.tray.is_some();
    app.model.visible |= !app.model.tray;
    let visible = app.model.visible;
    app.last_visibility = Some(visible);
    eframe::run_native(
        "dex2pc",
        eframe::NativeOptions {
            // Mesa's Wayland EGL swap can wait indefinitely for vblank. The UI
            // already schedules idle repaints every 100 ms, so it need not block here.
            vsync: false,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([420.0, 520.0])
                .with_visible(visible),
            ..Default::default()
        },
        Box::new(move |cc| {
            app.font_notice = (!fonts::install(&cc.egui_ctx)).then_some(fonts::NOTICE);
            Ok(Box::new(app))
        }),
    )
    .map_err(|err| io::Error::other(err.to_string()))?;
    Ok(())
}

fn icons(dir: &std::path::Path) -> io::Result<PathBuf> {
    let icons = dir.join("icons");
    std::fs::create_dir_all(&icons)?;
    for (name, bytes) in [
        ("idle", include_str!("../../assets/dex2pc-idle.svg")),
        (
            "connecting",
            include_str!("../../assets/dex2pc-connecting.svg"),
        ),
        (
            "projecting",
            include_str!("../../assets/dex2pc-projecting.svg"),
        ),
        ("error", include_str!("../../assets/dex2pc-error.svg")),
    ] {
        std::fs::write(icons.join(format!("dex2pc-{name}.svg")), bytes)?;
    }
    Ok(icons)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_app_starts_with_visible_window() {
        let model = Model::new(
            PathBuf::from("/unused"),
            Ok(Some(Options {
                mode: Some(Mode::Usb),
                quality: Some(Quality::High),
                ..Options::default()
            })),
        );
        assert!(model.visible);
    }
    #[test]
    fn unconfigured_app_opens_wizard() {
        let model = Model::new(PathBuf::from("/unused"), Ok(None));
        assert!(model.visible);
    }
    #[test]
    fn invalid_config_is_displayed_in_wizard() {
        let model = Model::new(
            PathBuf::from("/cfg"),
            Err("設定檔 /cfg 的欄位 mode：要是 usb 或 wifi".into()),
        );
        assert_eq!(
            model.error.as_deref(),
            Some("設定檔 /cfg 的欄位 mode：要是 usb 或 wifi")
        );
    }
    #[test]
    fn tray_window_close_keeps_session_alive() {
        let mut model = Model::new(PathBuf::from("/unused"), Ok(None));
        model.tray = true;
        model.close_window();
        assert!(!model.session.quitting);
        assert!(!model.visible);
    }
    #[test]
    fn no_tray_window_close_requests_stop_and_exit() {
        let mut model = Model::new(PathBuf::from("/unused"), Ok(None));
        model.close_window();
        assert!(model.session.quitting);
        assert!(model.session.stopping);
    }
    #[test]
    fn fake_ipc_show_reveals_hidden_window() {
        let (tx, rx) = mpsc::channel();
        let mut model = Model::new(PathBuf::from("/unused"), Ok(None));
        model.visible = false;
        // 假 IPC 只送相同事件，不啟動 socket 或視窗。
        tx.send(Event::Action(Action::Show)).unwrap();
        let Event::Action(action) = rx.recv().unwrap() else {
            panic!("非預期事件");
        };
        model.choose(action);
        assert!(model.visible);
    }
    #[test]
    fn tray_quality_action_persists_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut model = Model::new(path.clone(), Ok(None));
        model.choose(Action::Quality(Quality::Low));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "quality = \"low\"\n"
        );
    }
    #[test]
    fn language_action_persists_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut model = Model::new(path.clone(), Ok(None));
        model.choose(Action::Language(Language::En));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "language = \"en\"\nquality = \"balanced\"\n"
        );
    }
    #[test]
    fn low_quality_maps_to_four_megabit() {
        assert_eq!(Quality::Low.bitrate(), "4M");
    }
    #[test]
    fn balanced_quality_maps_to_eight_megabit() {
        assert_eq!(Quality::Balanced.bitrate(), "8M");
    }
    #[test]
    fn high_quality_maps_to_sixteen_megabit() {
        assert_eq!(Quality::High.bitrate(), "16M");
    }
    #[derive(Clone)]
    struct KillRunner(Sender<&'static str>);
    struct WaitingChild(Sender<&'static str>, &'static str);
    impl crate::runner::RunningCommand for WaitingChild {
        fn try_wait(&mut self) -> io::Result<Option<crate::runner::CommandOutput>> {
            Ok(None)
        }
        fn kill(&mut self) -> io::Result<()> {
            let _ = self.0.send(self.1);
            Ok(())
        }
    }
    impl CommandRunner for KillRunner {
        fn run(&self, _: &str, _: &[&str]) -> io::Result<crate::runner::CommandOutput> {
            unreachable!()
        }
        fn spawn(
            &self,
            program: &str,
            _: &[&str],
        ) -> io::Result<Box<dyn crate::runner::RunningCommand>> {
            Ok(Box::new(WaitingChild(
                self.0.clone(),
                if program == "scrcpy" { "scrcpy" } else { "adb" },
            )))
        }
    }
    fn headless_app() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let (tx, events) = mpsc::channel();
        let instance = ipc::acquire(dir.path(), tx.clone()).unwrap().unwrap();
        (
            App {
                last_visibility: Some(true),
                model: Model::new(dir.path().join("config.toml"), Ok(None)),
                events,
                tx,
                command: Vec::new(),
                tray: None,
                _instance: instance,
                busy: false,
                checked: false,
                next_check: Instant::now(),
                install: Vec::new(),
                environment_done: false,
                load_error: None,
                font_notice: None,
                interface: controller::Interface::default(),
            },
            dir,
        )
    }
    #[test]
    fn starting_projection_hides_window_when_tray_is_available() {
        let (mut app, _dir) = headless_app();
        app.model.tray = true;
        let (killed, _rx) = mpsc::channel();
        let runner = worker::SessionRunner::with_backend(app.tx.clone(), KillRunner(killed));
        let _child = runner.spawn("scrcpy", &["--new-display"]).unwrap();
        app.pump(&egui::Context::default());
        assert!(!app.model.visible);
    }
    #[test]
    fn starting_projection_keeps_window_visible_without_tray() {
        let (mut app, _dir) = headless_app();
        let (killed, _rx) = mpsc::channel();
        let runner = worker::SessionRunner::with_backend(app.tx.clone(), KillRunner(killed));
        let _child = runner.spawn("scrcpy", &["--new-display"]).unwrap();
        app.pump(&egui::Context::default());
        assert!(app.model.visible);
    }
    #[test]
    fn usb_ready_event_saves_mode_and_requests_start() {
        let (mut app, _dir) = headless_app();
        app.environment_done = true;
        app.wireless_event(controller::WirelessEvent::Usb(Ok("SM_S918B".into())));
        assert!(matches!(
            app.events.try_recv().unwrap(),
            Event::LaunchSession
        ));
        assert_eq!(
            std::fs::read_to_string(&app.model.path).unwrap(),
            "mode = \"usb\"\nquality = \"balanced\"\n"
        );
    }
    struct QrRunner;
    impl CommandRunner for QrRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<crate::runner::CommandOutput> {
            assert_eq!(program, "adb");
            let stdout = match args {
                ["connect", "192.168.18.59:37001"] => "connected to 192.168.18.59:37001",
                ["devices", "-l"] => "192.168.18.59:37001 device model:SM_S918B",
                _ => panic!("unexpected command"),
            };
            Ok(crate::runner::CommandOutput {
                code: Some(0),
                stdout: stdout.into(),
                stderr: String::new(),
            })
        }
        fn run_timeout(
            &self,
            program: &str,
            args: &[&str],
            _: Duration,
        ) -> io::Result<crate::runner::TimedOutput> {
            assert_eq!(program, "adb");
            let stdout = match args {
                ["pair", "192.168.18.59:44517", "test-secret"] => {
                    "Successfully paired to 192.168.18.59:44517"
                }
                ["mdns", "services"] => "adb-phone-new _adb-tls-connect._tcp 192.168.18.59:37001",
                _ => panic!("unexpected command"),
            };
            Ok(crate::runner::TimedOutput::Exited(
                crate::runner::CommandOutput {
                    code: Some(0),
                    stdout: stdout.into(),
                    stderr: String::new(),
                },
            ))
        }
    }
    #[test]
    fn qr_pair_success_connects_and_requests_session_start() {
        let (mut app, _dir) = headless_app();
        wireless::pair_qr(&QrRunner, "192.168.18.59:44517", "test-secret").unwrap();
        let (_tx, cancel) = mpsc::channel();
        let service = wireless::discover(&QrRunner, &cancel, 1, |entries| {
            wireless::choose(entries, Some("192.168.18.59:44517"), None)
        })
        .unwrap();
        let result =
            controller::connect_service(&QrRunner, service, &app.model.path, &mut Vec::new());
        app.model.session.running = true;
        app.wireless_event(controller::WirelessEvent::Connected(result));
        assert!(matches!(
            app.events.try_recv().unwrap(),
            Event::LaunchSession
        ));
        assert_eq!(
            std::fs::read_to_string(&app.model.path).unwrap(),
            "mode = \"wifi\"\nquality = \"balanced\"\nwifi_address = \"192.168.18.59:37001\"\n"
        );
    }
    #[test]
    fn language_setting_loads_after_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut model = Model::new(path.clone(), Ok(None));
        model.choose(Action::Language(Language::ZhTw));
        assert_eq!(
            config::load(&path).unwrap().unwrap().language,
            Some(Language::ZhTw)
        );
    }
    #[test]
    fn clear_resets_stop_before_starting_disconnect_worker() {
        let (mut app, _dir) = headless_app();
        app.interface.clear_requested = true;
        app.model.session.stopping = true;
        app.deferred_setup_actions();
        let stopping = app.model.session.stopping;
        // Join the config-only worker before dropping its temporary directory.
        assert!(matches!(
            app.events.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::Cleared(Ok(()))
        ));
        assert!(!stopping);
    }
    #[test]
    fn clear_requested_during_running_session_waits_before_saving() {
        let (mut app, _dir) = headless_app();
        app.model.session.running = true;
        app.clear_connection();
        app.deferred_setup_actions();
        assert!(app.interface.clear_requested);
        assert!(!app.model.path.exists());
    }
    #[test]
    fn failed_discovery_exposes_rescan_and_repair() {
        let (mut app, _dir) = headless_app();
        app.model.session.running = true;
        app.wireless_event(controller::WirelessEvent::Connected(Err(
            wireless::NOT_FOUND.into(),
        )));
        assert!(app.interface.fallback);
    }
    #[test]
    fn unchanged_visible_window_does_not_request_remapping() {
        assert_eq!(visibility_command(true, &mut Some(true)), None);
    }
    #[test]
    fn first_run_model_is_visible() {
        assert!(Model::new(PathBuf::from("/tmp/example/config.toml"), Ok(None)).visible);
    }
    #[test]
    fn closing_to_tray_requests_hiding() {
        assert_eq!(visibility_command(false, &mut Some(true)), Some(false));
    }
    #[test]
    fn showing_from_tray_requests_mapping() {
        assert_eq!(visibility_command(true, &mut Some(false)), Some(true));
    }
    #[test]
    fn failed_discovery_finishes_the_session() {
        let (mut app, _dir) = headless_app();
        app.model.session.running = true;
        app.wireless_event(controller::WirelessEvent::Connected(Err(
            wireless::NOT_FOUND.into(),
        )));
        assert_eq!(
            (app.model.session.running, app.model.status),
            (false, Status::Error)
        );
    }
    #[test]
    fn stopped_discovery_does_not_show_an_error() {
        let (mut app, _dir) = headless_app();
        app.model.session.running = true;
        app.model.session.stopping = true;
        app.wireless_event(controller::WirelessEvent::Connected(Err(
            "已停止連線".into()
        )));
        assert_eq!((app.model.status, app.model.error), (Status::Idle, None));
    }
    #[test]
    fn stop_kills_scrcpy_even_when_probe_is_latest_command() {
        let (mut app, _dir) = headless_app();
        let (killed, rx) = mpsc::channel();
        let runner = worker::SessionRunner::with_backend(app.tx.clone(), KillRunner(killed));
        let _scrcpy = runner.spawn("scrcpy", &["--new-display"]).unwrap();
        let _probe = runner
            .spawn("adb", &["-s", "phone:5555", "shell", "true"])
            .unwrap();
        app.pump(&egui::Context::default());
        app.action(Action::Stop);
        let mut killed = [
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        ];
        killed.sort_unstable();
        assert_eq!(killed, ["adb", "scrcpy"]);
    }
    #[test]
    fn stop_during_wait_for_device_finishes_idle() {
        let (mut app, _dir) = headless_app();
        let (killed, _rx) = mpsc::channel();
        let runner = worker::SessionRunner::with_backend(app.tx.clone(), KillRunner(killed));
        let mut wait = runner
            .spawn("adb", &["-s", "phone", "wait-for-device"])
            .unwrap();
        app.model.session.running = true;
        app.pump(&egui::Context::default());
        app.action(Action::Stop);
        assert_eq!(wait.try_wait().unwrap().unwrap().code, None);
        app.tx.send(Event::Finished(Status::Error, None)).unwrap();
        app.pump(&egui::Context::default());
        assert_eq!(app.model.status, Status::Idle);
    }
    #[test]
    fn finished_error_without_reason_gets_a_recovery_message() {
        let (mut app, _dir) = headless_app();
        app.tx.send(Event::Finished(Status::Error, None)).unwrap();
        app.pump(&egui::Context::default());
        assert_eq!(
            app.model.error.as_deref(),
            Some("操作失敗，請檢查手機連線與環境後重試。")
        );
    }
    #[test]
    fn stop_during_wifi_connect_finishes_idle() {
        let (mut app, _dir) = headless_app();
        let (killed, rx) = mpsc::channel();
        let runner = worker::SessionRunner::with_backend(app.tx.clone(), KillRunner(killed));
        let mut connect = runner.spawn("adb", &["connect", "phone:5555"]).unwrap();
        app.model.session.running = true;
        app.pump(&egui::Context::default());
        app.action(Action::Stop);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), "adb");
        assert_eq!(connect.try_wait().unwrap().unwrap().code, None);
        app.tx
            .send(Event::Finished(Status::Error, Some("連線中止".into())))
            .unwrap();
        app.pump(&egui::Context::default());
        assert_eq!((app.model.status, app.model.error), (Status::Idle, None));
    }
    #[test]
    fn successful_usb_check_does_not_erase_a_session_error() {
        let (mut app, _dir) = headless_app();
        app.model
            .fail("投影失敗（exit code 7）。請檢查手機連線與環境，依下方訊息處理後重試。".into());
        app.tx.send(Event::Checked(Ok("phone".into()))).unwrap();
        app.pump(&egui::Context::default());
        assert_eq!(
            app.model.error.as_deref(),
            Some("投影失敗（exit code 7）。請檢查手機連線與環境，依下方訊息處理後重試。")
        );
    }
    #[test]
    fn missing_environment_tools_explain_the_install_command() {
        let (mut app, _dir) = headless_app();
        app.tx
            .send(Event::Environment(vec!["sudo apt install adb".into()]))
            .unwrap();
        app.pump(&egui::Context::default());
        assert_eq!(
            app.model.error.as_deref(),
            Some("缺少必要工具或版本不符，請安裝後按「重新檢查環境」：\nsudo apt install adb")
        );
    }
    #[test]
    fn tray_start_before_environment_ready_explains_the_blocker() {
        let (mut app, _dir) = headless_app();
        app.model.options.mode = Some(Mode::Usb);
        app.action(Action::Start);
        assert_eq!(
            app.model.error.as_deref(),
            Some("環境尚未就緒，請重新檢查環境並依安裝提示處理。")
        );
    }
    #[test]
    fn tray_failure_opens_the_window_to_show_its_reason() {
        let (mut app, _dir) = headless_app();
        app.model.visible = false;
        app.tx
            .send(Event::Finished(
                Status::Error,
                Some("投影失敗，請重新連線。".into()),
            ))
            .unwrap();
        app.pump(&egui::Context::default());
        assert!(app.model.visible);
    }
}
