//! 版號只寫在 Cargo.toml。README badge 由 `cargo run --example sync_version_badge` 產生，
//! 這裡檢查它是最新的產物；CHANGELOG 每次進版要有對應的段落，進版漏寫時測試失敗。

// 和 example 共用同一份產生邏輯，測試不另寫一份 badge 格式；`main` 在這裡用不到
#[allow(dead_code)]
#[path = "../examples/sync_version_badge.rs"]
mod sync_version_badge;

use sync_version_badge::sync_badge;

const README: &str = include_str!("../README.md");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");

#[test]
fn readme_version_badge_is_up_to_date_with_cargo_toml() {
    assert_eq!(
        sync_badge(README, env!("CARGO_PKG_VERSION")).as_deref(),
        Some(README),
        "README 的版本 badge 不是最新的，請執行 `cargo run --example sync_version_badge`"
    );
}

#[test]
fn sync_badge_replaces_only_the_badge_line() {
    assert_eq!(
        sync_badge(
            "# t\n\n![version](https://img.shields.io/badge/version-0.1.0-blue)\n![x](y)\n",
            "0.2.0"
        ),
        Some(
            "# t\n\n![version](https://img.shields.io/badge/version-0.2.0-blue)\n![x](y)\n"
                .to_owned()
        )
    );
}

#[test]
fn sync_badge_doubles_dash_in_prerelease_version() {
    assert_eq!(
        sync_badge(
            "![version](https://img.shields.io/badge/version-0.1.0-blue)\n",
            "0.2.0-rc.1"
        ),
        Some("![version](https://img.shields.io/badge/version-0.2.0--rc.1-blue)\n".to_owned())
    );
}

#[test]
fn sync_badge_without_badge_line_is_none() {
    assert_eq!(sync_badge("# t\n", "0.2.0"), None);
}

#[test]
fn changelog_has_section_for_cargo_toml_version() {
    assert!(CHANGELOG.contains(concat!("## [", env!("CARGO_PKG_VERSION"), "]")));
}

/// 取出從 `heading` 那一行到下一個同級或更高層標題之前的文字，找不到標題時回傳空字串。
fn section<'a>(doc: &'a str, heading: &str) -> &'a str {
    let Some(start) = doc.find(heading) else {
        return "";
    };
    let body = &doc[start + heading.len()..];
    match body.find("\n## ") {
        Some(end) => &body[..end],
        None => body,
    }
}

#[test]
fn readme_quick_start_asks_for_usb_debugging_data_cable_and_start_command() {
    let quick_start = section(README, "## 快速開始");

    assert!(quick_start.contains("USB 偵錯"));
    assert!(quick_start.contains("支援資料傳輸的 USB-C 線"));
    assert!(quick_start.contains("dex2pc start"));
}

#[test]
fn readme_acceptance_list_names_mouse_and_keyboard_checks() {
    let acceptance = section(README, "## 實機驗收清單");

    // 這兩項是任務的核心需求（滑鼠點擊、鍵盤輸入），沒列進清單就等於沒有追蹤
    assert!(acceptance.contains("**滑鼠**"));
    assert!(acceptance.contains("**鍵盤**"));
}

#[test]
fn readme_acceptance_list_is_not_empty() {
    let acceptance = section(README, "## 實機驗收清單");

    assert!(acceptance.contains("- ["));
}

#[test]
fn version_is_zero_x_test_release() {
    // F10：版號進 0.x 測試版，1.0 之前由使用者決定何時升 MAJOR
    assert!(env!("CARGO_PKG_VERSION").starts_with("0."));
}
