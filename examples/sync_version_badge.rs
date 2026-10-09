//! 用 Cargo.toml 的版號重新產生 README 的版本 badge：`cargo run --example sync_version_badge`。
//!
//! 版號只寫在 Cargo.toml；README 的 badge 是這支程式的產物，不要手改。
//! `tests/docs.rs` 會檢查 README 是不是這支程式產生的最新結果。

use std::fs;
use std::path::Path;
use std::process::ExitCode;

/// badge 那一行的開頭，用來在 README 裡找到要替換的行。
pub const BADGE_PREFIX: &str = "![version](https://img.shields.io/badge/version-";

/// 回傳 `readme` 把版本 badge 那一行換成 `version` 之後的內容；README 沒有 badge 行時回傳 `None`。
///
/// shields.io 的路徑用 `-` 分隔欄位，版號裡的 `-`（例如 `0.2.0-rc.1`）要寫成 `--`。
#[must_use]
pub fn sync_badge(readme: &str, version: &str) -> Option<String> {
    let badge = format!("{BADGE_PREFIX}{}-blue)", version.replace('-', "--"));
    let mut found = false;
    let synced = readme
        .split_inclusive('\n')
        .map(|line| {
            if line.starts_with(BADGE_PREFIX) {
                found = true;
                let newline = if line.ends_with('\n') { "\n" } else { "" };
                format!("{badge}{newline}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    found.then_some(synced)
}

fn main() -> ExitCode {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let result = fs::read_to_string(&path).and_then(|readme| {
        let Some(synced) = sync_badge(&readme, env!("CARGO_PKG_VERSION")) else {
            return Ok(false);
        };
        fs::write(&path, synced)?;
        Ok(true)
    });
    match result {
        Ok(true) => {
            println!(
                "{} 的版本 badge 已更新為 {}",
                path.display(),
                env!("CARGO_PKG_VERSION")
            );
            ExitCode::SUCCESS
        }
        Ok(false) => {
            eprintln!("{} 找不到以 {BADGE_PREFIX} 開頭的 badge 行", path.display());
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("{}：{err}", path.display());
            ExitCode::FAILURE
        }
    }
}
