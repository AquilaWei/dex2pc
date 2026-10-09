use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

/// 虛擬顯示器的解析度，例如 `1920x1080`；寬高都必須是正整數。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolution {
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

impl FromStr for Resolution {
    type Err = InvalidDisplay;

    /// 解析 `寬x高`（`x` 不分大小寫）。
    ///
    /// # Errors
    /// 缺少 `x`、寬或高不是正整數（0、負數、非數字）時回傳 `InvalidDisplay::Resolution`。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidDisplay::Resolution(s.to_string());
        let (width, height) = s.trim().split_once(['x', 'X']).ok_or_else(invalid)?;
        Ok(Self {
            width: width.parse().map_err(|_| invalid())?,
            height: height.parse().map_err(|_| invalid())?,
        })
    }
}

/// 虛擬顯示器的 DPI，決定 DeX 介面的縮放；必須是正整數。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dpi(pub NonZeroU32);

impl fmt::Display for Dpi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Dpi {
    type Err = InvalidDisplay;

    /// # Errors
    /// 不是正整數（0、負數、非數字）時回傳 `InvalidDisplay::Dpi`。
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim()
            .parse()
            .map(Self)
            .map_err(|_| InvalidDisplay::Dpi(s.to_string()))
    }
}

/// 解析度或 DPI 的字串不合法；`Display` 是給使用者看的說明，含原始輸入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidDisplay {
    Resolution(String),
    Dpi(String),
}

impl fmt::Display for InvalidDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolution(raw) => write!(
                f,
                "解析度「{raw}」不合法：格式為 寬x高，寬高都要是正整數，例如 1920x1080"
            ),
            Self::Dpi(raw) => write!(f, "DPI「{raw}」不合法：要是正整數，例如 160"),
        }
    }
}

impl std::error::Error for InvalidDisplay {}

/// scrcpy 的 `--keyboard` / `--mouse` 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// 經 Android API 注入事件（scrcpy 預設）。
    Sdk,
    /// 在手機上模擬實體 HID 裝置；鍵盤配置由手機決定，較適合中文輸入法。
    Uhid,
    /// 經 USB AOA 協定模擬 HID，只能在 USB 連線下使用。
    Aoa,
    /// 不轉送這類事件。
    Disabled,
}

impl InputMode {
    fn as_arg(self) -> &'static str {
        match self {
            Self::Sdk => "sdk",
            Self::Uhid => "uhid",
            Self::Aoa => "aoa",
            Self::Disabled => "disabled",
        }
    }
}

/// 啟動 DeX 視窗用的 scrcpy 選項；`Default` 是沒有設定檔與 CLI 參數時的內建值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub resolution: Resolution,
    pub dpi: Dpi,
    pub window_title: String,
    /// 例如 `16M`；`None` 時不加參數，沿用 scrcpy 預設（8M）。原樣傳給 scrcpy，由它檢查格式。
    pub video_bit_rate: Option<String>,
    pub audio: bool,
    /// `None` 時不加參數，沿用 scrcpy 預設。
    pub keyboard: Option<InputMode>,
    /// `None` 時不加參數，沿用 scrcpy 預設。
    pub mouse: Option<InputMode>,
}

impl Default for Options {
    /// 1920x1080/160 是暫定值，等實機回饋 DeX 介面大小後再調。
    fn default() -> Self {
        Self {
            resolution: Resolution {
                width: NonZeroU32::new(1920).unwrap(),
                height: NonZeroU32::new(1080).unwrap(),
            },
            dpi: Dpi(NonZeroU32::new(160).unwrap()),
            window_title: "Samsung DeX".to_string(),
            video_bit_rate: None,
            audio: true,
            keyboard: None,
            mouse: None,
        }
    }
}

/// 組出 scrcpy 的參數（不含程式名稱 `scrcpy`），讓 DeX 開在新的虛擬顯示器上並成為電腦上的一個視窗。
///
/// `serial` 是 `device::select` 選出的裝置；給了就加 `--serial`，
/// 避免連多台手機時 scrcpy 自己報錯。不合法的解析度與 DPI 在解析 `Resolution`、`Dpi` 時就被擋下，
/// 所以這裡不會失敗。
pub fn build_args(options: &Options, serial: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(serial) = serial {
        args.push(format!("--serial={serial}"));
    }
    args.push(format!(
        "--new-display={}/{}",
        options.resolution, options.dpi
    ));
    args.push(format!("--window-title={}", options.window_title));
    if let Some(bit_rate) = &options.video_bit_rate {
        args.push(format!("--video-bit-rate={bit_rate}"));
    }
    if !options.audio {
        args.push("--no-audio".to_string());
    }
    if let Some(mode) = options.keyboard {
        args.push(format!("--keyboard={}", mode.as_arg()));
    }
    if let Some(mode) = options.mouse {
        args.push(format!("--mouse={}", mode.as_arg()));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_with_defaults_creates_1080p_dex_display() {
        let args = build_args(&Options::default(), None);

        assert_eq!(
            args,
            ["--new-display=1920x1080/160", "--window-title=Samsung DeX"]
        );
    }

    #[test]
    fn build_args_with_serial_adds_serial_first() {
        let args = build_args(&Options::default(), Some("R5CW1234ABC"));

        assert_eq!(
            args,
            [
                "--serial=R5CW1234ABC",
                "--new-display=1920x1080/160",
                "--window-title=Samsung DeX",
            ]
        );
    }

    #[test]
    fn build_args_with_uhid_keyboard_adds_keyboard_mode() {
        let options = Options {
            keyboard: Some(InputMode::Uhid),
            ..Options::default()
        };

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=1920x1080/160",
                "--window-title=Samsung DeX",
                "--keyboard=uhid",
            ]
        );
    }

    #[test]
    fn build_args_with_uhid_mouse_adds_mouse_mode() {
        let options = Options {
            mouse: Some(InputMode::Uhid),
            ..Options::default()
        };

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=1920x1080/160",
                "--window-title=Samsung DeX",
                "--mouse=uhid",
            ]
        );
    }

    #[test]
    fn build_args_with_audio_off_adds_no_audio() {
        let options = Options {
            audio: false,
            ..Options::default()
        };

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=1920x1080/160",
                "--window-title=Samsung DeX",
                "--no-audio",
            ]
        );
    }

    #[test]
    fn build_args_with_16m_bit_rate_adds_video_bit_rate() {
        let options = Options {
            video_bit_rate: Some("16M".to_string()),
            ..Options::default()
        };

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=1920x1080/160",
                "--window-title=Samsung DeX",
                "--video-bit-rate=16M",
            ]
        );
    }

    #[test]
    fn build_args_with_all_options_set_includes_every_flag() {
        let options = Options {
            resolution: "2560x1440".parse().unwrap(),
            dpi: "284".parse().unwrap(),
            window_title: "DeX".to_string(),
            video_bit_rate: Some("20M".to_string()),
            audio: false,
            keyboard: Some(InputMode::Aoa),
            mouse: Some(InputMode::Sdk),
        };

        assert_eq!(
            build_args(&options, Some("R5CW1234ABC")),
            [
                "--serial=R5CW1234ABC",
                "--new-display=2560x1440/284",
                "--window-title=DeX",
                "--video-bit-rate=20M",
                "--no-audio",
                "--keyboard=aoa",
                "--mouse=sdk",
            ]
        );
    }

    #[test]
    fn build_args_with_disabled_keyboard_passes_disabled() {
        let options = Options {
            keyboard: Some(InputMode::Disabled),
            ..Options::default()
        };

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=1920x1080/160",
                "--window-title=Samsung DeX",
                "--keyboard=disabled",
            ]
        );
    }

    #[test]
    fn resolution_parses_width_and_height() {
        let resolution: Resolution = "2560x1440".parse().unwrap();

        assert_eq!(resolution.to_string(), "2560x1440");
    }

    #[test]
    fn resolution_accepts_uppercase_x() {
        let resolution: Resolution = "2560X1440".parse().unwrap();

        assert_eq!(resolution.to_string(), "2560x1440");
    }

    #[test]
    fn resolution_with_zero_width_is_rejected() {
        assert_eq!(
            "0x1080".parse::<Resolution>(),
            Err(InvalidDisplay::Resolution("0x1080".to_string()))
        );
    }

    #[test]
    fn resolution_with_zero_height_is_rejected() {
        assert_eq!(
            "1920x0".parse::<Resolution>(),
            Err(InvalidDisplay::Resolution("1920x0".to_string()))
        );
    }

    #[test]
    fn resolution_with_negative_width_is_rejected() {
        assert_eq!(
            "-1920x1080".parse::<Resolution>(),
            Err(InvalidDisplay::Resolution("-1920x1080".to_string()))
        );
    }

    #[test]
    fn resolution_with_non_numeric_height_is_rejected() {
        assert_eq!(
            "1920xabc".parse::<Resolution>(),
            Err(InvalidDisplay::Resolution("1920xabc".to_string()))
        );
    }

    #[test]
    fn resolution_without_separator_is_rejected() {
        assert_eq!(
            "1920".parse::<Resolution>(),
            Err(InvalidDisplay::Resolution("1920".to_string()))
        );
    }

    #[test]
    fn dpi_parses_positive_integer() {
        let dpi: Dpi = "284".parse().unwrap();

        assert_eq!(dpi.to_string(), "284");
    }

    #[test]
    fn dpi_zero_is_rejected() {
        assert_eq!(
            "0".parse::<Dpi>(),
            Err(InvalidDisplay::Dpi("0".to_string()))
        );
    }

    #[test]
    fn dpi_negative_is_rejected() {
        assert_eq!(
            "-160".parse::<Dpi>(),
            Err(InvalidDisplay::Dpi("-160".to_string()))
        );
    }

    #[test]
    fn dpi_non_numeric_is_rejected() {
        assert_eq!(
            "high".parse::<Dpi>(),
            Err(InvalidDisplay::Dpi("high".to_string()))
        );
    }

    #[test]
    fn invalid_resolution_message_shows_input_and_expected_format() {
        let err = InvalidDisplay::Resolution("0x1080".to_string());

        assert_eq!(
            err.to_string(),
            "解析度「0x1080」不合法：格式為 寬x高，寬高都要是正整數，例如 1920x1080"
        );
    }

    #[test]
    fn invalid_dpi_message_shows_input_and_expected_format() {
        let err = InvalidDisplay::Dpi("0".to_string());

        assert_eq!(err.to_string(), "DPI「0」不合法：要是正整數，例如 160");
    }
}
