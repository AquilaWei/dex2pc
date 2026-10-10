use std::fmt;

/// DeX 改建在 Android 16 Desktop Mode 上、能跑在虛擬顯示器上的最低 One UI 版本。
pub const MIN_ONE_UI: OneUi = OneUi(8, 0);

/// One UI 主、次版號；欄位順序讓 derive 出來的比較就是版本大小的比較。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OneUi(pub u32, pub u32);

impl fmt::Display for OneUi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

/// 解析 `ro.build.version.oneui`：格式為主版號 ×10000 + 次版號 ×100 + 修訂號，
/// 例如 `80000` → 8.0、`70100` → 7.1（修訂號不影響判定，捨去）。
///
/// 不是純數字（含空字串）時回傳 `None`。
pub fn parse_one_ui(raw: &str) -> Option<OneUi> {
    let code: u32 = raw.trim().parse().ok()?;
    Some(OneUi(code / 10000, code / 100 % 100))
}

/// 從 `adb shell getprop`（不帶參數）的輸出讀取的裝置資訊。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// `ro.product.manufacturer`，例如 `samsung`。
    pub manufacturer: Option<String>,
    /// `ro.product.model`，例如 `SM-S918B`。
    pub model: Option<String>,
    /// `ro.build.version.sdk`，例如 `36`。
    pub sdk: Option<String>,
    /// `ro.build.version.oneui` 的原始值；非 Samsung 裝置沒有這個屬性。
    pub one_ui: Option<String>,
}

impl DeviceInfo {
    /// 解析 `getprop` 的 `[key]: [value]` 列；值跨多行或格式不符的列略過，空值視為沒有。
    pub fn from_getprop(output: &str) -> Self {
        let get = |key: &str| {
            output
                .lines()
                .filter_map(parse_prop_line)
                .find(|(k, _)| *k == key)
                .and_then(|(_, v)| (!v.is_empty()).then(|| v.to_string()))
        };
        Self {
            manufacturer: get("ro.product.manufacturer"),
            model: get("ro.product.model"),
            sdk: get("ro.build.version.sdk"),
            one_ui: get("ro.build.version.oneui"),
        }
    }
}

fn parse_prop_line(line: &str) -> Option<(&str, &str)> {
    let inner = line.trim().strip_prefix('[')?.strip_suffix(']')?;
    inner.split_once("]: [")
}

/// 裝置不能以 DeX 虛擬顯示模式使用的原因；`Display` 是給使用者看的完整說明。
#[derive(Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// 不是 Samsung（或沒有 One UI 版本屬性），沒有 DeX。
    NotSamsung {
        manufacturer: Option<String>,
    },
    /// Samsung，但 `ro.build.version.oneui` 讀不到或不是數字。
    UnknownOneUi {
        raw: Option<String>,
    },
    TooOld(OneUi),
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSamsung { manufacturer } => write!(
                f,
                "不支援：這不是 Samsung 裝置（製造商：{}），沒有 DeX",
                manufacturer.as_deref().unwrap_or("未知")
            ),
            Self::UnknownOneUi { raw: None } => write!(
                f,
                "不支援：讀不到 One UI 版本（ro.build.version.oneui），需要 One UI {MIN_ONE_UI} 以上"
            ),
            Self::UnknownOneUi { raw: Some(raw) } => write!(
                f,
                "不支援：無法解析 One UI 版本 {raw:?}，需要 One UI {MIN_ONE_UI} 以上"
            ),
            Self::TooOld(version) => write!(
                f,
                "不支援：One UI {version} 的 DeX 無法在虛擬顯示器上執行，需要 One UI {MIN_ONE_UI} 以上"
            ),
        }
    }
}

impl std::error::Error for Unsupported {}

/// 判定裝置能否以 DeX 虛擬顯示模式使用，可以時回傳 One UI 版本。
///
/// # Errors
/// 不是 Samsung、讀不到 One UI 版本，或版本低於 [`MIN_ONE_UI`] 時回傳對應的 [`Unsupported`]。
pub fn check(info: &DeviceInfo) -> Result<OneUi, Unsupported> {
    let is_samsung = info
        .manufacturer
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("samsung"));
    if !is_samsung {
        return Err(Unsupported::NotSamsung {
            manufacturer: info.manufacturer.clone(),
        });
    }
    let version = info
        .one_ui
        .as_deref()
        .and_then(parse_one_ui)
        .ok_or_else(|| Unsupported::UnknownOneUi {
            raw: info.one_ui.clone(),
        })?;
    if version < MIN_ONE_UI {
        return Err(Unsupported::TooOld(version));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    const S23_ONE_UI_80: &str = "[ro.build.version.oneui]: [80000]\n[ro.build.version.sdk]: [36]\n[ro.product.manufacturer]: [samsung]\n[ro.product.model]: [SM-S918B]\n";
    const PIXEL: &str = "[ro.build.version.sdk]: [36]\n[ro.product.manufacturer]: [Google]\n[ro.product.model]: [Pixel 9]\n";

    fn samsung(one_ui: Option<&str>) -> DeviceInfo {
        DeviceInfo {
            manufacturer: Some("samsung".to_string()),
            model: Some("SM-S918B".to_string()),
            sdk: Some("35".to_string()),
            one_ui: one_ui.map(str::to_string),
        }
    }

    #[test]
    fn parse_one_ui_80000_is_8_0() {
        assert_eq!(parse_one_ui("80000"), Some(OneUi(8, 0)));
    }

    #[test]
    fn parse_one_ui_70100_is_7_1() {
        assert_eq!(parse_one_ui("70100"), Some(OneUi(7, 1)));
    }

    #[test]
    fn parse_one_ui_ignores_patch_digits() {
        assert_eq!(parse_one_ui("80501"), Some(OneUi(8, 5)));
    }

    #[test]
    fn parse_one_ui_rejects_non_numeric() {
        assert_eq!(parse_one_ui("abc"), None);
    }

    #[test]
    fn from_getprop_reads_samsung_properties() {
        assert_eq!(
            DeviceInfo::from_getprop(S23_ONE_UI_80),
            DeviceInfo {
                manufacturer: Some("samsung".to_string()),
                model: Some("SM-S918B".to_string()),
                sdk: Some("36".to_string()),
                one_ui: Some("80000".to_string()),
            }
        );
    }

    #[test]
    fn from_getprop_without_oneui_property_leaves_it_empty() {
        assert_eq!(DeviceInfo::from_getprop(PIXEL).one_ui, None);
    }

    #[test]
    fn from_getprop_treats_empty_value_as_missing() {
        let info = DeviceInfo::from_getprop("[ro.build.version.oneui]: []\n");

        assert_eq!(info.one_ui, None);
    }

    #[test]
    fn check_samsung_one_ui_8_0_is_supported() {
        assert_eq!(check(&samsung(Some("80000"))), Ok(OneUi(8, 0)));
    }

    #[test]
    fn check_accepts_upper_case_manufacturer() {
        let info = DeviceInfo {
            manufacturer: Some("Samsung".to_string()),
            ..samsung(Some("80000"))
        };

        assert_eq!(check(&info), Ok(OneUi(8, 0)));
    }

    #[test]
    fn check_samsung_one_ui_7_1_is_unsupported_and_asks_for_8() {
        let err = check(&samsung(Some("70100"))).unwrap_err();

        assert_eq!(err, Unsupported::TooOld(OneUi(7, 1)));
        assert_eq!(
            err.to_string(),
            "不支援：One UI 7.1 的 DeX 無法在虛擬顯示器上執行，需要 One UI 8.0 以上"
        );
    }

    #[test]
    fn check_non_samsung_without_oneui_is_unsupported_and_names_manufacturer() {
        let err = check(&DeviceInfo::from_getprop(PIXEL)).unwrap_err();

        assert_eq!(
            err.to_string(),
            "不支援：這不是 Samsung 裝置（製造商：Google），沒有 DeX"
        );
    }

    #[test]
    fn check_samsung_without_oneui_property_is_unsupported() {
        let err = check(&samsung(None)).unwrap_err();

        assert_eq!(
            err.to_string(),
            "不支援：讀不到 One UI 版本（ro.build.version.oneui），需要 One UI 8.0 以上"
        );
    }

    #[test]
    fn check_samsung_with_garbled_oneui_is_unsupported() {
        let err = check(&samsung(Some("eight"))).unwrap_err();

        assert_eq!(
            err,
            Unsupported::UnknownOneUi {
                raw: Some("eight".to_string())
            }
        );
    }
}
