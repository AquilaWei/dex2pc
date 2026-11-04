mod ipc;
mod tray;
mod view;
mod worker;

use crate::preferences::{Mode, Quality};
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
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "未連線",
            Self::Connecting => "連線中",
            Self::Projecting => "投影中",
            Self::Error => "錯誤",
        }
    }
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
    Mode(Mode),
    Quality(Quality),
    SwitchWifi,
    Show,
    Quit,
}
enum Event {
    Action(Action),
    Command(Sender<()>, bool),
    Log(String),
    Finished(Status, Option<String>),
    Checked(Result<String, String>),
    Switched(Result<String, String>),
    Paired(Result<(), String>),
    TrayLost,
    Environment(Vec<String>),
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
        let configured = options.mode.is_some()
            && options.quality.is_some()
            && (options.mode != Some(Mode::Wifi) || options.wifi_address.is_some());
        Self {
            options,
            path,
            status: Status::Idle,
            session: Lifecycle::default(),
            visible: !configured,
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
    fn save(&mut self) -> bool {
        match config::save_gui(&self.path, &self.options) {
            Ok(()) => true,
            Err(err) => {
                self.error = Some(format!("{}：{err}", self.path.display()));
                self.status = Status::Error;
                false
            }
        }
    }
    fn choose(&mut self, action: Action) {
        match action {
            Action::Mode(mode) => {
                self.options.mode = Some(mode);
                self.save();
            }
            Action::Quality(quality) => {
                self.options.quality = Some(quality);
                self.save();
            }
            Action::Show => self.visible = true,
            Action::Stop => self.session.stopping = true,
            Action::Quit => {
                self.session.stopping = true;
                self.session.quitting = true;
            }
            Action::Start | Action::SwitchWifi => {}
        }
    }
}

struct App {
    model: Model,
    events: Receiver<Event>,
    tx: Sender<Event>,
    command: Option<Sender<()>>,
    tray: Option<ksni::blocking::Handle<tray::Tray>>,
    _instance: ipc::Instance,
    address: String,
    pair_address: String,
    code: String,
    busy: bool,
    checked: bool,
    next_check: Instant,
    install: Vec<String>,
    environment_done: bool,
    load_error: Option<String>,
}
impl App {
    fn action(&mut self, action: Action) {
        self.model.choose(action);
        match action {
            Action::Start
                if !self.model.session.running
                    && !self.busy
                    && self.environment_done
                    && self.install.is_empty() =>
            {
                if !self.model.save() {
                    return;
                }
                self.model.session.running = true;
                self.model.session.stopping = false;
                self.model.status = Status::Connecting;
                worker::session(self.tx.clone(), self.model.options.clone());
            }
            Action::Stop | Action::Quit => {
                if let Some(tx) = &self.command {
                    let _ = tx.send(());
                }
            }
            Action::SwitchWifi if !self.busy && !self.model.session.running => {
                self.busy = true;
                let tx = self.tx.clone();
                let path = self.model.path.clone();
                thread::spawn(move || {
                    let runner = worker::SessionRunner::new(tx.clone());
                    let result = crate::wifi::connect_and_save(
                        &runner,
                        None,
                        None,
                        &path,
                        &mut thread::sleep,
                        &mut io::sink(),
                    )
                    .and_then(|()| {
                        config::load(&path)
                            .map_err(io::Error::other)?
                            .and_then(|options| options.wifi_address)
                            .ok_or_else(|| io::Error::other("未取得 Wi-Fi 位址"))
                    });
                    let _ = tx.send(Event::Switched(result.map_err(|err| err.to_string())));
                });
            }
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
                    self.command = Some(tx);
                    if projection {
                        self.model.status = Status::Projecting;
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
                    self.model.session.running = false;
                    self.model.status = if self.model.session.stopping {
                        Status::Idle
                    } else {
                        status
                    };
                    self.model.error = error;
                    self.command = None;
                }
                Event::Checked(result) => {
                    self.busy = false;
                    if !self.model.session.quitting {
                        self.model.session.stopping = false;
                    }
                    self.checked = result.is_ok();
                    self.model.error = result.err();
                    self.command = None;
                }
                Event::Switched(result) => {
                    self.busy = false;
                    self.command = None;
                    match result {
                        Ok(address) => {
                            self.address.clone_from(&address);
                            self.model.options.wifi_address = Some(address);
                            self.model.options.mode = Some(Mode::Wifi);
                            self.model.save();
                        }
                        Err(err) => {
                            self.model.error = Some(err);
                            self.model.status = Status::Error;
                        }
                    }
                }
                Event::Paired(result) => {
                    self.busy = false;
                    self.command = None;
                    self.model.error = result.err();
                }
                Event::Environment(commands) => {
                    self.install = commands;
                    self.environment_done = true;
                    self.busy = false;
                }
                Event::TrayLost => {
                    self.model.tray = false;
                    self.tray = None;
                    self.model.visible = true;
                }
            }
        }
        if let Some(handle) = &self.tray {
            let mode = self.model.options.mode.unwrap_or_default();
            let quality = self.model.options.quality.unwrap_or_default();
            let status = self.model.status;
            handle.update(|tray| {
                tray.mode = mode;
                tray.quality = quality;
                tray.status = status;
            });
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.model.visible));
        if self.model.session.quitting && !self.model.session.running && !self.busy {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
                    .map(|device| device.serial.clone())
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

/// 非互動配對不經過任何 log；失敗只回傳固定訊息，避免 adb 的輸出回顯配對碼。
fn pair(runner: &dyn CommandRunner, address: &str, code: &str) -> Result<(), String> {
    crate::wifi::validate(address)?;
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err("配對碼必須是六位數字".into());
    }
    let output = runner
        .run("adb", &["pair", address, code])
        .map_err(|_| "無法執行 adb 配對".to_string())?;
    if output.success() {
        Ok(())
    } else {
        Err("配對失敗：請重開手機配對視窗並使用新的埠與碼，或改用 USB 切換".into())
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
                "sudo apt install scrcpy（需 3.2 以上；Debian 請依 README 安裝官方版）"
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
        mode: model.options.mode.unwrap_or_default(),
        quality: model.options.quality.unwrap_or_default(),
        status: model.status,
        icons: icons.to_string_lossy().into_owned(),
    }
    .spawn()
    .ok();
    let address = model.options.wifi_address.clone().unwrap_or_default();
    let mut app = App {
        model,
        events,
        tx,
        command: None,
        tray,
        _instance: instance,
        address,
        pair_address: String::new(),
        code: String::new(),
        busy: true,
        checked: false,
        next_check: Instant::now(),
        install: Vec::new(),
        environment_done: false,
        load_error: None,
    };
    app.load_error.clone_from(&app.model.error);
    app.environment();
    app.model.tray = app.tray.is_some();
    app.model.visible |= !app.model.tray;
    let visible = app.model.visible;
    eframe::run_native(
        "dex2pc 設定",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([640.0, 660.0])
                .with_visible(visible),
            ..Default::default()
        },
        Box::new(move |cc| {
            load_fonts(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
    .map_err(|err| io::Error::other(err.to_string()))?;
    Ok(())
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
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
    struct FakeRunner;
    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<crate::runner::CommandOutput> {
            assert_eq!(program, "adb");
            assert_eq!(args, ["pair", "phone:1234", "123456"]);
            Ok(crate::runner::CommandOutput {
                code: Some(1),
                stdout: "123456".into(),
                stderr: "123456".into(),
            })
        }
    }
    #[test]
    fn pairing_failure_never_exposes_code_from_adb_output() {
        assert_eq!(
            pair(&FakeRunner, "phone:1234", "123456"),
            Err("配對失敗：請重開手機配對視窗並使用新的埠與碼，或改用 USB 切換".into())
        );
    }
    #[test]
    fn invalid_pair_address_does_not_execute_adb() {
        assert!(
            pair(&FakeRunner, "invalid", "123456")
                .unwrap_err()
                .contains("無效位址")
        );
    }
    #[test]
    fn configured_app_starts_with_hidden_window() {
        let model = Model::new(
            PathBuf::from("/unused"),
            Ok(Some(Options {
                mode: Some(Mode::Usb),
                quality: Some(Quality::High),
                ..Options::default()
            })),
        );
        assert!(!model.visible);
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
            "mode = \"usb\"\nquality = \"low\"\n"
        );
    }
    #[test]
    fn tray_mode_action_persists_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut model = Model::new(path.clone(), Ok(None));
        model.choose(Action::Mode(Mode::Wifi));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "mode = \"wifi\"\nquality = \"balanced\"\n"
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
}
