mod capability;
mod config;
mod desktop;
mod device;
mod doctor;
mod runner;
mod scrcpy;
mod start;
mod usb;

use std::env;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::runner::SystemRunner;

/// 透過 USB-C 線把 Samsung DeX 以視窗形式顯示在 Linux 電腦上（底層使用 scrcpy）
#[derive(Parser)]
// `version` 不給值時 clap 取 CARGO_PKG_VERSION，版號只寫在 Cargo.toml
#[command(name = "dex2pc", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 檢查 adb 與 scrcpy（3.2 以上）是否就緒、手機是否已連接並授權、是否為 One UI 8 以上的 Samsung，缺少時印出 Fedora 的安裝指令
    Doctor {
        /// 要使用的裝置序號（adb devices 列出的第一欄）；只連一台時可省略
        #[arg(long, short)]
        serial: Option<String>,
    },
    /// 檢查環境後以 DeX 虛擬顯示模式啟動 scrcpy，DeX 會開成電腦上的一個視窗；關閉視窗即結束
    Start {
        /// 要使用的裝置序號（adb devices 列出的第一欄）；只連一台時可省略
        #[arg(long, short)]
        serial: Option<String>,
        /// 只印出會執行的 scrcpy 指令，不檢查環境也不執行任何外部指令
        #[arg(long)]
        dry_run: bool,
        /// USB 線拔除或斷線時等手機重新接上並自動重新啟動；關閉視窗或按 Ctrl+C 結束，斷線後連續 5 次重新啟動失敗就放棄
        #[arg(long)]
        watch: bool,
        #[command(flatten)]
        overrides: config::Overrides,
    },
    /// 顯示設定檔位置與合併 CLI 參數後實際生效的設定；設定檔是 XDG_CONFIG_HOME 下的 dex2pc/config.toml，未設定時用 ~/.config
    Config {
        /// 只印設定檔路徑
        #[arg(long)]
        path: bool,
        #[command(flatten)]
        overrides: config::Overrides,
    },
    /// 建立應用程式選單捷徑 dex2pc.desktop（放在 XDG_DATA_HOME 下的 applications，未設定時用 ~/.local/share/applications），點它等同執行 dex2pc start
    InstallDesktop {
        /// 刪除已建立的捷徑
        #[arg(long)]
        uninstall: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor { serial } => {
            match doctor::run(
                &SystemRunner,
                serial.as_deref(),
                Path::new("/sys"),
                &mut io::stdout(),
            ) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(err) => {
                    eprintln!("dex2pc: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Start {
            serial,
            dry_run,
            watch,
            overrides,
        } => match start(serial.as_deref(), dry_run, watch, overrides) {
            Ok(code) => ExitCode::from(code),
            Err(err) => {
                eprintln!("dex2pc: {err}");
                ExitCode::FAILURE
            }
        },
        Command::Config { path, overrides } => match show_config(path, overrides) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("dex2pc: {err}");
                ExitCode::FAILURE
            }
        },
        Command::InstallDesktop { uninstall } => match install_desktop(uninstall) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("dex2pc: {err}");
                ExitCode::FAILURE
            }
        },
    }
}

/// 讀設定檔並套用 CLI 覆寫後啟動；回傳 dex2pc 的 exit code（scrcpy 失敗時沿用它的 code）。
fn start(
    serial: Option<&str>,
    dry_run: bool,
    watch: bool,
    overrides: config::Overrides,
) -> Result<u8, Box<dyn std::error::Error>> {
    let path = config::path(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))?;
    let options = overrides.apply(config::load(&path)?.unwrap_or_default());
    if dry_run {
        start::dry_run(&options, serial, &mut io::stdout())?;
        return Ok(0);
    }
    Ok(start::run(
        &SystemRunner,
        &options,
        serial,
        watch.then(start::Watch::default).as_ref(),
        Path::new("/sys"),
        &mut io::stdout(),
    )?)
}

fn show_config(
    path_only: bool,
    overrides: config::Overrides,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config::path(env::var_os("XDG_CONFIG_HOME"), env::var_os("HOME"))?;
    if path_only {
        println!("{}", path.display());
        return Ok(());
    }
    let loaded = config::load(&path)?;
    let from_file = loaded.is_some();
    let options = overrides.apply(loaded.unwrap_or_default());
    config::show(&mut io::stdout(), &path, from_file, &options)?;
    Ok(())
}

fn install_desktop(uninstall: bool) -> Result<(), Box<dyn std::error::Error>> {
    let dir = desktop::applications_dir(env::var_os("XDG_DATA_HOME"), env::var_os("HOME"))?;
    if uninstall {
        match desktop::uninstall(&dir)? {
            Some(path) => println!("已刪除 {}", path.display()),
            None => println!(
                "沒有捷徑需要刪除（{} 不存在）",
                dir.join("dex2pc.desktop").display()
            ),
        }
        return Ok(());
    }
    // 捷徑寫入目前執行檔的絕對路徑；用 cargo run 執行時會指向 target/ 裡的檔案
    let path = desktop::install(&dir, &env::current_exe()?)?;
    println!("已建立 {}", path.display());
    Ok(())
}
