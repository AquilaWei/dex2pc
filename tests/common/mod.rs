//! 打包測試共用：檢查外部工具在不在。本機缺工具就略過，設了 `CI` 環境變數就失敗，
//! 避免 CI 因為少裝工具而悄悄跳過整組測試。

use std::path::Path;

fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    // 只查檔案在不在，不執行它（有些工具如 rpm2cpio 沒有安全的 --version）
    std::env::split_paths(&path).any(|dir| Path::new(&dir).join(program).is_file())
}

/// 全部工具都找得到時回傳 `true`；否則本機印出缺少的工具並回傳 `false`（呼叫端略過測試），
/// CI 上則直接讓測試失敗。
pub fn require_tools(programs: &[&str]) -> bool {
    let missing: Vec<&str> = programs
        .iter()
        .copied()
        .filter(|program| !on_path(program))
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(std::env::var_os("CI").is_none(), "CI 需要安裝：{missing:?}");
    eprintln!("缺少 {missing:?}，略過");
    false
}
