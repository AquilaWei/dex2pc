use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Args;

use crate::scrcpy::{Dpi, InputMode, Options, Resolution};

/// 設定檔中可用的欄位；出現其他欄位就報錯，避免打錯字的設定被默默忽略。
const FIELDS: [&str; 7] = [
    "resolution",
    "dpi",
    "video_bit_rate",
    "audio",
    "keyboard",
    "mouse",
    "window_title",
];

/// 算出設定檔位置：`$XDG_CONFIG_HOME/dex2pc/config.toml`，沒設時用 `$HOME/.config/dex2pc/config.toml`。
///
/// 環境變數由呼叫端讀好傳入，測試不用改行程的全域環境變數。
/// 依 XDG Base Directory 規範，空字串或相對路徑的 `XDG_CONFIG_HOME` 視同未設定。
///
/// # Errors
/// `XDG_CONFIG_HOME` 不能用且 `HOME` 沒設（或是空字串）時回傳 `ConfigError::NoConfigDir`。
pub fn path(
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, ConfigError> {
    let base = match xdg_config_home.map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => match home {
            Some(home) if !home.is_empty() => PathBuf::from(home).join(".config"),
            _ => return Err(ConfigError::NoConfigDir),
        },
    };
    Ok(base.join("dex2pc").join("config.toml"))
}

/// 讀取設定檔並套在內建預設值上；檔案不存在時回傳 `None`，由呼叫端改用 `Options::default()`。
///
/// # Errors
/// 檔案存在但讀不到（例如權限不足）、TOML 語法錯誤、有未知欄位、欄位型別或值不合法時回傳錯誤，
/// 錯誤訊息含檔案路徑與欄位名稱。
pub fn load(path: &Path) -> Result<Option<Options>, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(ConfigError::Read {
                path: path.to_path_buf(),
                reason: err.to_string(),
            });
        }
    };
    parse(&text, path).map(Some)
}

/// 把設定檔內容套在 `Options::default()` 上；`path` 只用在錯誤訊息。
fn parse(text: &str, path: &Path) -> Result<Options, ConfigError> {
    let table: toml::Table = text
        .parse()
        .map_err(|err: toml::de::Error| ConfigError::Syntax {
            path: path.to_path_buf(),
            reason: err.to_string(),
        })?;
    let mut options = Options::default();
    for (field, value) in &table {
        let invalid = |reason: String| ConfigError::Field {
            path: path.to_path_buf(),
            field: field.clone(),
            reason,
        };
        match field.as_str() {
            "resolution" => {
                options.resolution = value
                    .as_str()
                    .ok_or_else(|| invalid("要是字串，例如 \"1920x1080\"".to_string()))?
                    .parse::<Resolution>()
                    .map_err(|err| invalid(err.to_string()))?;
            }
            "dpi" => {
                let dpi = value
                    .as_integer()
                    .ok_or_else(|| invalid("要是整數，例如 160".to_string()))?;
                // 經過 Dpi::from_str，0 與負數的錯誤訊息和 CLI 的 --dpi 一致
                options.dpi = dpi
                    .to_string()
                    .parse::<Dpi>()
                    .map_err(|err| invalid(err.to_string()))?;
            }
            "video_bit_rate" => {
                options.video_bit_rate = Some(
                    value
                        .as_str()
                        .ok_or_else(|| invalid("要是字串，例如 \"16M\"".to_string()))?
                        .to_string(),
                );
            }
            "audio" => {
                options.audio = value
                    .as_bool()
                    .ok_or_else(|| invalid("要是 true 或 false".to_string()))?;
            }
            "keyboard" | "mouse" => {
                let mode = value
                    .as_str()
                    .ok_or_else(|| invalid("要是字串，例如 \"uhid\"".to_string()))?
                    .parse::<InputMode>()
                    .map_err(|err| invalid(err.to_string()))?;
                if field == "keyboard" {
                    options.keyboard = Some(mode);
                } else {
                    options.mouse = Some(mode);
                }
            }
            "window_title" => {
                options.window_title = value
                    .as_str()
                    .ok_or_else(|| invalid("要是字串".to_string()))?
                    .to_string();
            }
            _ => {
                return Err(ConfigError::UnknownField {
                    path: path.to_path_buf(),
                    field: field.clone(),
                });
            }
        }
    }
    Ok(options)
}

/// 可覆寫設定檔的 CLI 參數；沒給的參數沿用設定檔（或內建預設值）。
#[derive(Debug, Default, Args)]
pub struct Overrides {
    /// 虛擬顯示器解析度，例如 2560x1440
    #[arg(long)]
    pub resolution: Option<Resolution>,
    /// 虛擬顯示器 DPI，數字越大介面越大，例如 160
    #[arg(long)]
    pub dpi: Option<Dpi>,
    /// 影像位元率，例如 16M；小字模糊時調高
    #[arg(long)]
    pub video_bit_rate: Option<String>,
    /// 不轉送手機音訊
    #[arg(long)]
    pub no_audio: bool,
    /// 鍵盤模式：sdk、uhid、aoa、disabled
    #[arg(long)]
    pub keyboard: Option<InputMode>,
    /// 滑鼠模式：sdk、uhid、aoa、disabled
    #[arg(long)]
    pub mouse: Option<InputMode>,
    /// 電腦上視窗的標題
    #[arg(long)]
    pub window_title: Option<String>,
}

impl Overrides {
    /// 把有給的 CLI 參數蓋到 `options` 上。`--no-audio` 只能關掉音訊，不能把設定檔的 `audio = false` 打開。
    pub fn apply(self, mut options: Options) -> Options {
        if let Some(resolution) = self.resolution {
            options.resolution = resolution;
        }
        if let Some(dpi) = self.dpi {
            options.dpi = dpi;
        }
        if self.video_bit_rate.is_some() {
            options.video_bit_rate = self.video_bit_rate;
        }
        if self.no_audio {
            options.audio = false;
        }
        if self.keyboard.is_some() {
            options.keyboard = self.keyboard;
        }
        if self.mouse.is_some() {
            options.mouse = self.mouse;
        }
        if let Some(title) = self.window_title {
            options.window_title = title;
        }
        options
    }
}

/// 印出設定檔位置與最後生效的設定，讓使用者確認設定檔有被讀到。
///
/// # Errors
/// 寫入 `out` 失敗時回傳 I/O 錯誤。
pub fn show(
    out: &mut impl Write,
    path: &Path,
    from_file: bool,
    options: &Options,
) -> io::Result<()> {
    let source = if from_file {
        ""
    } else {
        "（不存在，使用內建預設值）"
    };
    let scrcpy_default = "（scrcpy 預設）".to_string();
    let mode = |mode: Option<InputMode>| mode.map_or(scrcpy_default.clone(), |m| m.to_string());
    writeln!(out, "設定檔：{}{source}", path.display())?;
    writeln!(out, "resolution = {}", options.resolution)?;
    writeln!(out, "dpi = {}", options.dpi)?;
    writeln!(
        out,
        "video_bit_rate = {}",
        options.video_bit_rate.as_ref().unwrap_or(&scrcpy_default)
    )?;
    writeln!(out, "audio = {}", options.audio)?;
    writeln!(out, "keyboard = {}", mode(options.keyboard))?;
    writeln!(out, "mouse = {}", mode(options.mouse))?;
    writeln!(out, "window_title = {}", options.window_title)
}

/// 找不到或讀不懂設定檔；`Display` 是給使用者看的訊息，含檔案路徑與欄位名稱。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NoConfigDir,
    Read {
        path: PathBuf,
        reason: String,
    },
    Syntax {
        path: PathBuf,
        reason: String,
    },
    UnknownField {
        path: PathBuf,
        field: String,
    },
    Field {
        path: PathBuf,
        field: String,
        reason: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDir => {
                write!(f, "找不到設定目錄：XDG_CONFIG_HOME 與 HOME 都沒有設定")
            }
            Self::Read { path, reason } => {
                write!(f, "無法讀取設定檔 {}：{reason}", path.display())
            }
            Self::Syntax { path, reason } => {
                write!(f, "設定檔 {} 不是合法的 TOML：\n{reason}", path.display())
            }
            Self::UnknownField { path, field } => write!(
                f,
                "設定檔 {} 有未知欄位 {field}；可用的欄位：{}",
                path.display(),
                FIELDS.join("、")
            ),
            Self::Field {
                path,
                field,
                reason,
            } => write!(f, "設定檔 {} 的欄位 {field}：{reason}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::scrcpy::build_args;

    fn write_config(dir: &tempfile::TempDir, text: &str) -> PathBuf {
        let path = dir.path().join("config.toml");
        fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn path_uses_xdg_config_home_when_set() {
        let path = path(Some("/xdg".into()), Some("/home/user".into()));

        assert_eq!(path, Ok(PathBuf::from("/xdg/dex2pc/config.toml")));
    }

    #[test]
    fn path_without_xdg_config_home_uses_home_dot_config() {
        let path = path(None, Some("/home/user".into()));

        assert_eq!(
            path,
            Ok(PathBuf::from("/home/user/.config/dex2pc/config.toml"))
        );
    }

    #[test]
    fn path_with_empty_xdg_config_home_falls_back_to_home() {
        let path = path(Some("".into()), Some("/home/user".into()));

        assert_eq!(
            path,
            Ok(PathBuf::from("/home/user/.config/dex2pc/config.toml"))
        );
    }

    #[test]
    fn path_with_relative_xdg_config_home_falls_back_to_home() {
        let path = path(Some("relative/dir".into()), Some("/home/user".into()));

        assert_eq!(
            path,
            Ok(PathBuf::from("/home/user/.config/dex2pc/config.toml"))
        );
    }

    #[test]
    fn path_without_xdg_or_home_is_an_error() {
        assert_eq!(path(None, None), Err(ConfigError::NoConfigDir));
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();

        let loaded = load(&dir.path().join("config.toml"));

        assert_eq!(loaded, Ok(None));
    }

    #[test]
    fn load_empty_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "");

        assert_eq!(load(&path), Ok(Some(Options::default())));
    }

    #[test]
    fn load_resolution_is_reflected_in_scrcpy_args() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "resolution = '2560x1440'\n");

        let options = load(&path).unwrap().unwrap();

        assert_eq!(
            build_args(&options, None),
            ["--new-display=2560x1440/160", "--window-title=Samsung DeX"]
        );
    }

    #[test]
    fn load_reads_every_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(
            &dir,
            "resolution = \"2560x1440\"\n\
             dpi = 284\n\
             video_bit_rate = \"16M\"\n\
             audio = false\n\
             keyboard = \"uhid\"\n\
             mouse = \"aoa\"\n\
             window_title = \"DeX\"\n",
        );

        let options = load(&path).unwrap().unwrap();

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=2560x1440/284",
                "--window-title=DeX",
                "--video-bit-rate=16M",
                "--no-audio",
                "--keyboard=uhid",
                "--mouse=aoa",
            ]
        );
    }

    #[test]
    fn load_unknown_field_reports_path_and_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "resolutoin = \"2560x1440\"\n");

        assert_eq!(
            load(&path),
            Err(ConfigError::UnknownField {
                path: path.clone(),
                field: "resolutoin".to_string(),
            })
        );
    }

    #[test]
    fn load_dpi_as_string_reports_path_and_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "dpi = \"160\"\n");

        assert_eq!(
            load(&path),
            Err(ConfigError::Field {
                path: path.clone(),
                field: "dpi".to_string(),
                reason: "要是整數，例如 160".to_string(),
            })
        );
    }

    #[test]
    fn load_zero_dpi_reports_dpi_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "dpi = 0\n");

        assert_eq!(
            load(&path),
            Err(ConfigError::Field {
                path: path.clone(),
                field: "dpi".to_string(),
                reason: "DPI「0」不合法：要是正整數，例如 160".to_string(),
            })
        );
    }

    #[test]
    fn load_audio_as_string_reports_path_and_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "audio = \"no\"\n");

        assert_eq!(
            load(&path),
            Err(ConfigError::Field {
                path: path.clone(),
                field: "audio".to_string(),
                reason: "要是 true 或 false".to_string(),
            })
        );
    }

    #[test]
    fn load_invalid_resolution_reports_field_and_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "resolution = \"0x1080\"\n");

        assert_eq!(
            load(&path),
            Err(ConfigError::Field {
                path: path.clone(),
                field: "resolution".to_string(),
                reason: "解析度「0x1080」不合法：格式為 寬x高，寬高都要是正整數，例如 1920x1080"
                    .to_string(),
            })
        );
    }

    #[test]
    fn load_unknown_mouse_mode_reports_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "mouse = \"hid\"\n");

        assert_eq!(
            load(&path),
            Err(ConfigError::Field {
                path: path.clone(),
                field: "mouse".to_string(),
                reason: "鍵鼠模式「hid」不合法：可用 sdk、uhid、aoa、disabled".to_string(),
            })
        );
    }

    #[test]
    fn load_invalid_toml_reports_syntax_error_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config(&dir, "dpi = \n");

        let err = load(&path).unwrap_err();

        assert!(matches!(err, ConfigError::Syntax { path: p, .. } if p == path));
    }

    #[test]
    fn load_directory_instead_of_file_reports_read_error() {
        let dir = tempfile::tempdir().unwrap();

        let err = load(dir.path()).unwrap_err();

        assert!(matches!(err, ConfigError::Read { path: p, .. } if p == dir.path()));
    }

    #[test]
    fn unknown_field_message_shows_path_field_and_accepted_fields() {
        let err = ConfigError::UnknownField {
            path: PathBuf::from("/xdg/dex2pc/config.toml"),
            field: "resolutoin".to_string(),
        };

        assert_eq!(
            err.to_string(),
            "設定檔 /xdg/dex2pc/config.toml 有未知欄位 resolutoin；\
             可用的欄位：resolution、dpi、video_bit_rate、audio、keyboard、mouse、window_title"
        );
    }

    #[test]
    fn field_error_message_shows_path_and_field() {
        let err = ConfigError::Field {
            path: PathBuf::from("/xdg/dex2pc/config.toml"),
            field: "dpi".to_string(),
            reason: "要是整數，例如 160".to_string(),
        };

        assert_eq!(
            err.to_string(),
            "設定檔 /xdg/dex2pc/config.toml 的欄位 dpi：要是整數，例如 160"
        );
    }

    #[test]
    fn overrides_resolution_replaces_config_value() {
        let from_file = Options {
            resolution: "2560x1440".parse().unwrap(),
            ..Options::default()
        };
        let overrides = Overrides {
            resolution: Some("3840x2160".parse().unwrap()),
            ..Overrides::default()
        };

        let options = overrides.apply(from_file);

        assert_eq!(
            build_args(&options, None),
            ["--new-display=3840x2160/160", "--window-title=Samsung DeX"]
        );
    }

    #[test]
    fn overrides_without_flags_keep_config_values() {
        let from_file = Options {
            resolution: "2560x1440".parse().unwrap(),
            video_bit_rate: Some("16M".to_string()),
            keyboard: Some(InputMode::Uhid),
            ..Options::default()
        };

        let options = Overrides::default().apply(from_file.clone());

        assert_eq!(options, from_file);
    }

    #[test]
    fn overrides_every_flag_replaces_every_field() {
        let overrides = Overrides {
            resolution: Some("2560x1440".parse().unwrap()),
            dpi: Some("284".parse().unwrap()),
            video_bit_rate: Some("20M".to_string()),
            no_audio: true,
            keyboard: Some(InputMode::Uhid),
            mouse: Some(InputMode::Sdk),
            window_title: Some("DeX".to_string()),
        };

        let options = overrides.apply(Options::default());

        assert_eq!(
            build_args(&options, None),
            [
                "--new-display=2560x1440/284",
                "--window-title=DeX",
                "--video-bit-rate=20M",
                "--no-audio",
                "--keyboard=uhid",
                "--mouse=sdk",
            ]
        );
    }

    #[test]
    fn show_missing_file_prints_defaults_and_notes_missing_file() {
        let mut out = Vec::new();

        show(
            &mut out,
            Path::new("/xdg/dex2pc/config.toml"),
            false,
            &Options::default(),
        )
        .unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "設定檔：/xdg/dex2pc/config.toml（不存在，使用內建預設值）\n\
             resolution = 1920x1080\n\
             dpi = 160\n\
             video_bit_rate = （scrcpy 預設）\n\
             audio = true\n\
             keyboard = （scrcpy 預設）\n\
             mouse = （scrcpy 預設）\n\
             window_title = Samsung DeX\n"
        );
    }

    #[test]
    fn show_loaded_file_prints_set_values_without_missing_note() {
        let options = Options {
            video_bit_rate: Some("16M".to_string()),
            audio: false,
            keyboard: Some(InputMode::Uhid),
            mouse: Some(InputMode::Aoa),
            ..Options::default()
        };
        let mut out = Vec::new();

        show(
            &mut out,
            Path::new("/xdg/dex2pc/config.toml"),
            true,
            &options,
        )
        .unwrap();

        assert_eq!(
            String::from_utf8(out).unwrap(),
            "設定檔：/xdg/dex2pc/config.toml\n\
             resolution = 1920x1080\n\
             dpi = 160\n\
             video_bit_rate = 16M\n\
             audio = false\n\
             keyboard = uhid\n\
             mouse = aoa\n\
             window_title = Samsung DeX\n"
        );
    }
}
