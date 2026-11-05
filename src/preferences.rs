use std::fmt;
use std::str::FromStr;

/// GUI language; CLI output remains independent of this preference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    ZhTw,
    En,
}
impl Language {
    pub fn from_locale(lc_all: Option<&str>, lang: Option<&str>) -> Self {
        if lc_all
            .filter(|s| !s.is_empty())
            .or(lang)
            .unwrap_or("")
            .starts_with("zh")
        {
            Self::ZhTw
        } else {
            Self::En
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ZhTw => "zh-TW",
            Self::En => "en",
        }
    }
}
impl FromStr for Language {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "zh-TW" => Ok(Self::ZhTw),
            "en" => Ok(Self::En),
            _ => Err("要是 zh-TW 或 en".into()),
        }
    }
}

/// GUI 的傳輸方式；CLI 仍以自己的參數決定傳輸，不自動套用此選擇。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Usb,
    Wifi,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::Wifi => "wifi",
        }
    }
}
impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "usb" => Ok(Self::Usb),
            "wifi" => Ok(Self::Wifi),
            _ => Err("要是 usb 或 wifi".into()),
        }
    }
}

/// GUI 畫質只在 GUI 啟動時轉成 bitrate，避免改動既有 CLI 的選項優先順序。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    High,
    #[default]
    Balanced,
    Low,
}
impl Quality {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Balanced => "balanced",
            Self::Low => "low",
        }
    }
    pub fn bitrate(self) -> &'static str {
        match self {
            Self::High => "16M",
            Self::Balanced => "8M",
            Self::Low => "4M",
        }
    }
}
impl fmt::Display for Quality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for Quality {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "high" => Ok(Self::High),
            "balanced" => Ok(Self::Balanced),
            "low" => Ok(Self::Low),
            _ => Err("要是 high、balanced 或 low".into()),
        }
    }
}
