//! 進版時容易漏改 README badge 或 CHANGELOG，這裡讓它們和 Cargo.toml 的版號不一致時測試失敗。

const README: &str = include_str!("../README.md");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");

#[test]
fn readme_version_badge_matches_cargo_toml_version() {
    assert!(README.contains(concat!(
        "https://img.shields.io/badge/version-",
        env!("CARGO_PKG_VERSION"),
        "-blue"
    )));
}

#[test]
fn changelog_has_section_for_cargo_toml_version() {
    assert!(CHANGELOG.contains(concat!("## [", env!("CARGO_PKG_VERSION"), "]")));
}
