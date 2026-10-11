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

#[test]
fn readme_install_section_has_copyable_rpm_commands_with_copr_first() {
    let install = section(README, "\n## 安裝\n");

    let copr = install.find("sudo dnf copr enable zeno/scrcpy").unwrap();
    let rpm = install.find("sudo dnf install ./dex2pc-*.rpm").unwrap();
    assert!(copr < rpm, "COPR 要先於安裝 rpm");
}

#[test]
fn readme_install_section_says_rpm_is_fedora_x86_64_only_and_fails_without_copr() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("只支援 Fedora（x86_64）"));
    assert!(install.contains("沒有啟用 COPR 就安裝會失敗"));
}

#[test]
fn readme_install_section_explains_rpm_ships_the_menu_shortcut_unlike_cargo_install() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("不必再跑 `dex2pc install-desktop`"));
    assert!(install.contains("cargo install --path ."));
}

#[test]
fn readme_install_section_documents_building_the_rpm_and_removing_it() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("sudo dnf install rpm-build cargo rust desktop-file-utils git tar"));
    assert!(install.contains("packaging/build-rpm.sh"));
    assert!(install.contains("sudo dnf remove dex2pc"));
}

/// spec 的 `BuildRequires` 必須是這份清單。要改時兩處一起改：這份清單、spec，
/// 而且 README 的建置指令要涵蓋新加的套件（下面的 README 測試會檢查）
const RPM_BUILD_REQUIRES: [&str; 3] = ["cargo", "rust", "desktop-file-utils"];
/// `build-rpm.sh` 用到、但不在 spec 的 `BuildRequires` 裡的工具
const RPM_BUILD_TOOLS: [&str; 3] = ["rpm-build", "git", "tar"];

#[test]
fn readme_rpm_build_command_installs_the_tools_the_build_needs() {
    // 固定清單與 spec 的 BuildRequires 共用同一份常數，改常數就會同時影響 spec 測試，兩邊不會各自漂移
    let line = install_line(README, "sudo dnf install rpm-build");
    let installed: Vec<&str> = line.split_whitespace().collect();

    for tool in RPM_BUILD_REQUIRES.into_iter().chain(RPM_BUILD_TOOLS) {
        assert!(
            installed.contains(&tool),
            "{tool} 不在 README 的指令：{line}"
        );
    }
}

#[test]
fn spec_build_requires_are_exactly_the_list_the_readme_test_checks() {
    let spec = include_str!("../packaging/rpm/dex2pc.spec");

    let build_requires: Vec<&str> = spec
        .lines()
        .filter_map(|line| line.strip_prefix("BuildRequires:"))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();

    assert_eq!(build_requires, RPM_BUILD_REQUIRES);
}

/// 回傳含 `prefix` 的那一行（README 裡只出現一次的指令）
fn install_line<'a>(doc: &'a str, prefix: &str) -> &'a str {
    doc.lines()
        .find(|line| line.starts_with(prefix))
        .unwrap_or_else(|| panic!("找不到以 {prefix} 開頭的行"))
}

#[test]
fn readme_quick_start_uses_the_installed_dex2pc_command_not_cargo_run() {
    let quick_start = section(README, "\n## 快速開始\n");

    assert!(quick_start.contains("dex2pc start\n"));
    assert!(quick_start.contains("dex2pc doctor"));
    assert!(quick_start.contains("dex2pc start --dry-run"));
    assert!(quick_start.contains("dex2pc start --watch"));
    assert!(
        !quick_start.contains("cargo run -- start")
            && !quick_start.contains("cargo run --release -- start")
    );
    assert!(!quick_start.contains("cargo run -- doctor"));
}

#[test]
fn readme_keeps_cargo_run_in_the_development_section() {
    let development = section(README, "\n## 建置與執行\n");

    assert!(development.contains("開發用"));
    assert!(development.contains("cargo run -- doctor"));
}

#[test]
fn readme_install_section_says_patch_versions_are_artifacts_only() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("PATCH 版"));
    assert!(install.contains("不發佈 Release"));
    assert!(install.contains("artifact"));
}

#[test]
fn changelog_unreleased_announces_the_rpm_package_in_plain_words() {
    let unreleased = section(CHANGELOG, "\n## [Unreleased]\n");

    assert!(unreleased.contains("### 新增"));
    assert!(unreleased.contains("rpm 安裝包"));
    assert!(unreleased.contains("sudo dnf remove dex2pc"));
}

#[test]
fn readme_install_section_has_copyable_apt_install_for_the_deb() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("\nsudo apt install ./dex2pc_*.deb\n"));
}

#[test]
fn readme_install_section_says_deb_supports_only_debian_13_and_ubuntu_26_04_amd64() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("只支援 Debian 13（trixie）與 Ubuntu 26.04 以上，amd64"));
}

#[test]
fn readme_install_section_explains_getting_scrcpy_on_debian_13_before_installing_the_deb() {
    let install = section(README, "\n## 安裝\n");

    // F9 查到的倉庫版本：Debian 13 沒有 scrcpy，Ubuntu 26.04 是 3.3.4
    assert!(install.contains("倉庫**沒有 scrcpy**"));
    assert!(install.contains("倉庫的 scrcpy 是 3.3.4"));
    let official = install.find("scrcpy-linux-x86_64-v5.0.1.tar.gz").unwrap();
    let placeholder = install
        .find("sudo apt install ./scrcpy_5.0.1_all.deb")
        .unwrap();
    let dex2pc = install.rfind("sudo apt install ./dex2pc_*.deb").unwrap();
    assert!(official < placeholder && placeholder < dex2pc);
}

#[test]
fn readme_debian_scrcpy_download_url_is_the_official_5_0_1_static_build() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains(
        "wget https://github.com/Genymobile/scrcpy/releases/download/v5.0.1/scrcpy-linux-x86_64-v5.0.1.tar.gz\n"
    ));
}

#[test]
fn readme_debian_scrcpy_extracts_into_a_per_version_directory_and_links_to_it() {
    let install = section(README, "\n## 安裝\n");

    // 解壓到 /opt/scrcpy 底下以版本命名的目錄；升級時不會把新版塞進舊版目錄裡
    assert!(install.contains("sudo tar xf scrcpy-linux-x86_64-v5.0.1.tar.gz -C /opt/scrcpy\n"));
    assert!(install.contains(
        "sudo ln -sf /opt/scrcpy/scrcpy-linux-x86_64-v5.0.1/scrcpy /usr/local/bin/scrcpy\n"
    ));
    assert!(!install.contains("mv scrcpy-linux-x86_64"));
}

#[test]
fn readme_debian_scrcpy_placeholder_version_is_5_0_1() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("Package: scrcpy\\nVersion: 5.0.1\\n"));
}

#[test]
fn readme_debian_scrcpy_placeholder_deb_file_name_is_5_0_1() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("sudo apt install ./scrcpy_5.0.1_all.deb\n"));
}

#[test]
fn readme_debian_scrcpy_upgrade_relinks_and_checks_version_before_rebuilding_placeholder() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("**更新 scrcpy**"));
    assert!(install.contains("`ln -sf` 把連結改指向新版"));
    assert!(install.contains("`scrcpy --version` 確認是新版後再重建佔位套件"));
}

#[test]
fn readme_install_section_says_doctor_hints_are_fedora_only() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("`dex2pc doctor` 的安裝提示目前只列 Fedora 的 `dnf` 指令"));
}

#[test]
fn readme_install_section_documents_building_the_deb_and_removing_it() {
    let install = section(README, "\n## 安裝\n");

    assert!(install.contains("sudo apt install cargo git gzip binutils dpkg"));
    assert!(install.contains("packaging/build-deb.sh"));
    assert!(install.contains("sudo apt remove dex2pc"));
}

#[test]
fn changelog_unreleased_announces_the_deb_package_in_plain_words() {
    let unreleased = section(CHANGELOG, "\n## [Unreleased]\n");

    assert!(unreleased.contains("deb 安裝包"));
    assert!(unreleased.contains("sudo apt remove dex2pc"));
}
