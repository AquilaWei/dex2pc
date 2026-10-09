use std::process::ExitCode;

use clap::{Parser, Subcommand};

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
    /// 檢查 adb、scrcpy 與手機是否就緒（尚未實作）
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => {
            eprintln!("dex2pc: doctor 尚未實作");
            ExitCode::FAILURE
        }
    }
}
