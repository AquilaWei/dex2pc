use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 捷徑檔名固定，重複安裝時覆寫同一個檔案，不會產生重複的捷徑。
const FILE_NAME: &str = "dex2pc.desktop";

/// 算出捷徑目錄：`$XDG_DATA_HOME/applications`，沒設時用 `$HOME/.local/share/applications`。
///
/// 環境變數由呼叫端讀好傳入，測試不用改行程的全域環境變數。
/// 和 `config::path` 一樣，空字串或相對路徑的 `XDG_DATA_HOME` 依 XDG Base Directory 規範視同未設定。
///
/// # Errors
/// `XDG_DATA_HOME` 不能用且 `HOME` 沒設（或是空字串）時回傳 `DesktopError::NoDataDir`。
pub fn applications_dir(
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, DesktopError> {
    let base = match xdg_data_home.map(PathBuf::from) {
        Some(dir) if dir.is_absolute() => dir,
        _ => match home {
            Some(home) if !home.is_empty() => PathBuf::from(home).join(".local").join("share"),
            _ => return Err(DesktopError::NoDataDir),
        },
    };
    Ok(base.join("applications"))
}

/// 產生 `.desktop` 檔內容，`Exec` 是 `<exe> start`。
///
/// 用 dex2pc 執行檔的絕對路徑而不是指令名稱，因為從桌面環境啟動時的 PATH
/// 常常不含 `~/.cargo/bin`。不加 `--watch`：捷徑沒有終端機，`wait-for-device`
/// 等不到手機時會在背景一直等，使用者看不到也關不掉。
///
/// # Errors
/// 路徑不是 UTF-8 時回傳 `DesktopError::NonUtf8Path`（`.desktop` 檔規定是 UTF-8）；
/// 含有換行、tab、CR 以外的控制字元時回傳 `DesktopError::ControlCharInPath`
/// （string 值不能含控制字元，規範也只給這三個跳脫序列）。
pub fn entry(exe: &Path) -> Result<String, DesktopError> {
    let exe_str = exe
        .to_str()
        .ok_or_else(|| DesktopError::NonUtf8Path(exe.to_path_buf()))?;
    if exe_str
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t' | '\r'))
    {
        return Err(DesktopError::ControlCharInPath(exe.to_path_buf()));
    }
    Ok(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Samsung DeX\n\
         Comment=用 USB-C 線把 Samsung DeX 開成電腦上的視窗（dex2pc）\n\
         Exec={} start\n\
         Icon=video-display\n\
         Terminal=false\n\
         Categories=Utility;\n",
        quote_exec_arg(exe_str)
    ))
}

/// 依 Desktop Entry 規範把一個參數放進 `Exec`：一律加雙引號，引號內的 `"` `` ` `` `$` `\`
/// 先加反斜線跳脫；`Exec` 本身又是 string 型別，所以反斜線要再跳脫一次。`%` 是欄位代碼，寫成 `%%`。
/// 換行、tab、CR 不能原樣寫進檔案（換行會切斷這一行），用 string 型別的 `\n` `\t` `\r` 表示；
/// 讀取端還原後它們落在雙引號內，屬於參數的一部分。其他控制字元由 `entry` 事先拒絕。
fn quote_exec_arg(arg: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in arg.chars() {
        match ch {
            // 引號內跳脫成 `\\`，string 層再把兩個反斜線各跳脫一次
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(ch);
            }
            '%' => quoted.push_str("%%"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            '\r' => quoted.push_str("\\r"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

/// 在 `dir` 建立（或覆寫）`dex2pc.desktop`，目錄不存在時一併建立；回傳檔案路徑。
///
/// # Errors
/// `exe` 不是 UTF-8，或建立目錄、寫檔失敗（例如權限不足）時回傳錯誤。
pub fn install(dir: &Path, exe: &Path) -> Result<PathBuf, DesktopError> {
    let content = entry(exe)?;
    let path = dir.join(FILE_NAME);
    fs::create_dir_all(dir).map_err(|err| DesktopError::io(dir, &err))?;
    fs::write(&path, content).map_err(|err| DesktopError::io(&path, &err))?;
    Ok(path)
}

/// 刪除 `dir` 裡的 `dex2pc.desktop`；回傳刪除的檔案路徑，本來就沒有時回傳 `None`。
///
/// # Errors
/// 檔案存在但刪不掉（例如權限不足）時回傳錯誤。
pub fn uninstall(dir: &Path) -> Result<Option<PathBuf>, DesktopError> {
    let path = dir.join(FILE_NAME);
    match fs::remove_file(&path) {
        Ok(()) => Ok(Some(path)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(DesktopError::io(&path, &err)),
    }
}

/// 建立或刪除捷徑失敗；`Display` 是給使用者看的訊息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    NoDataDir,
    NonUtf8Path(PathBuf),
    ControlCharInPath(PathBuf),
    Io { path: PathBuf, reason: String },
}

impl DesktopError {
    fn io(path: &Path, err: &io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            reason: err.to_string(),
        }
    }
}

impl fmt::Display for DesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDataDir => write!(f, "找不到捷徑目錄：XDG_DATA_HOME 與 HOME 都沒有設定"),
            Self::NonUtf8Path(path) => write!(
                f,
                "dex2pc 的路徑 {} 不是 UTF-8，無法寫進捷徑",
                path.display()
            ),
            // 控制字元跳脫成 `\u{1b}` 再印，不直接送到終端機
            Self::ControlCharInPath(path) => write!(
                f,
                "dex2pc 的路徑 {} 含有控制字元，無法寫進捷徑",
                path.display().to_string().escape_debug()
            ),
            Self::Io { path, reason } => write!(f, "{}：{reason}", path.display()),
        }
    }
}

impl std::error::Error for DesktopError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applications_dir_uses_xdg_data_home_when_absolute() {
        assert_eq!(
            applications_dir(Some("/data".into()), Some("/home/u".into())),
            Ok(PathBuf::from("/data/applications"))
        );
    }

    #[test]
    fn applications_dir_falls_back_to_home_local_share_when_xdg_unset() {
        assert_eq!(
            applications_dir(None, Some("/home/u".into())),
            Ok(PathBuf::from("/home/u/.local/share/applications"))
        );
    }

    #[test]
    fn applications_dir_ignores_relative_xdg_data_home() {
        assert_eq!(
            applications_dir(Some("data".into()), Some("/home/u".into())),
            Ok(PathBuf::from("/home/u/.local/share/applications"))
        );
    }

    #[test]
    fn applications_dir_without_xdg_or_home_is_error() {
        assert_eq!(applications_dir(None, None), Err(DesktopError::NoDataDir));
    }

    #[test]
    fn entry_execs_dex2pc_start_by_absolute_path() {
        assert_eq!(
            entry(Path::new("/home/u/.cargo/bin/dex2pc")).unwrap(),
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Samsung DeX\n\
             Comment=用 USB-C 線把 Samsung DeX 開成電腦上的視窗（dex2pc）\n\
             Exec=\"/home/u/.cargo/bin/dex2pc\" start\n\
             Icon=video-display\n\
             Terminal=false\n\
             Categories=Utility;\n"
        );
    }

    #[test]
    fn entry_keeps_spaces_inside_quotes() {
        assert!(
            entry(Path::new("/opt/my tools/dex2pc"))
                .unwrap()
                .contains("Exec=\"/opt/my tools/dex2pc\" start\n")
        );
    }

    #[test]
    fn entry_escapes_reserved_characters_in_exec() {
        assert!(
            entry(Path::new("/opt/a\"b`c$d%e\\f/dex2pc"))
                .unwrap()
                .contains(r#"Exec="/opt/a\\"b\\`c\\$d%%e\\\\f/dex2pc" start"#)
        );
    }

    #[test]
    fn entry_escapes_newline_tab_and_carriage_return_in_exec() {
        assert!(
            entry(Path::new("/opt/a\nb\tc\rd/dex2pc"))
                .unwrap()
                .contains(r#"Exec="/opt/a\nb\tc\rd/dex2pc" start"#)
        );
    }

    #[test]
    fn entry_rejects_path_with_other_control_character() {
        assert_eq!(
            entry(Path::new("/opt/a\u{1b}b/dex2pc")),
            Err(DesktopError::ControlCharInPath(PathBuf::from(
                "/opt/a\u{1b}b/dex2pc"
            )))
        );
    }

    #[test]
    fn install_creates_missing_directory_and_writes_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("applications");

        let path = install(&dir, Path::new("/usr/bin/dex2pc")).unwrap();

        assert_eq!(path, dir.join("dex2pc.desktop"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            entry(Path::new("/usr/bin/dex2pc")).unwrap()
        );
    }

    #[test]
    fn install_twice_overwrites_single_file() {
        let tmp = tempfile::tempdir().unwrap();
        install(tmp.path(), Path::new("/old/dex2pc")).unwrap();

        install(tmp.path(), Path::new("/new/dex2pc")).unwrap();

        let names: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![OsString::from("dex2pc.desktop")]);
        assert!(
            fs::read_to_string(tmp.path().join("dex2pc.desktop"))
                .unwrap()
                .contains("Exec=\"/new/dex2pc\" start\n")
        );
    }

    #[test]
    fn uninstall_removes_installed_entry() {
        let tmp = tempfile::tempdir().unwrap();
        install(tmp.path(), Path::new("/usr/bin/dex2pc")).unwrap();

        let removed = uninstall(tmp.path()).unwrap();

        assert_eq!(removed, Some(tmp.path().join("dex2pc.desktop")));
        assert!(!tmp.path().join("dex2pc.desktop").exists());
    }

    #[test]
    fn uninstall_without_entry_returns_none() {
        let tmp = tempfile::tempdir().unwrap();

        assert_eq!(uninstall(tmp.path()), Ok(None));
    }
}
