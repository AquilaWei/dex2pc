mod capability;
mod config;
mod device;
mod doctor;
mod runner;
mod scrcpy;
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
    /// 檢查 adb 與 scrcpy（3.0 以上）是否就緒、手機是否已連接並授權、是否為 One UI 8 以上的 Samsung，缺少時印出 Fedora 的安裝指令
    Doctor {
        /// 要使用的裝置序號（adb devices 列出的第一欄）；只連一台時可省略
        #[arg(long, short)]
        serial: Option<String>,
    },
    /// 顯示設定檔位置與合併 CLI 參數後實際生效的設定；設定檔是 XDG_CONFIG_HOME 下的 dex2pc/config.toml，未設定時用 ~/.config
    Config {
        /// 只印設定檔路徑
        #[arg(long)]
        path: bool,
        #[command(flatten)]
        overrides: config::Overrides,
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
        Command::Config { path, overrides } => match show_config(path, overrides) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("dex2pc: {err}");
                ExitCode::FAILURE
            }
        },
    }
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
