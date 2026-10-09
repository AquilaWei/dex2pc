mod doctor;
mod runner;

use std::io;
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
    /// 檢查 adb 與 scrcpy（3.0 以上）是否就緒，缺少時印出 Fedora 的安裝指令
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => match doctor::run(&SystemRunner, &mut io::stdout()) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(err) => {
                eprintln!("dex2pc: {err}");
                ExitCode::FAILURE
            }
        },
    }
}
