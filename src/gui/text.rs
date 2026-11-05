use crate::preferences::Language;

macro_rules! translations {
    ($( $key:ident => ($zh:literal, $en:literal)),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub(super) enum Key { $($key),+ }
        pub(super) const TABLE: &[(&str, &str)] = &[$(($zh, $en)),+];
        pub(super) const fn get(language: Language, key: Key) -> &'static str {
            let pair = TABLE[key as usize];
            match language { Language::ZhTw => pair.0, Language::En => pair.1 }
        }
    };
}

translations! {
    Settings => ("設定", "Settings"),
    Title => ("讓手機，成為你的桌面。", "Your phone. Your desktop."),
    Choose => ("選擇連線方式，完成一次設定後即可一鍵開啟。", "Choose a connection. Set up once, then start with one click."),
    Usb => ("USB 連線", "USB connection"),
    Wifi => ("無線連線", "Wi-Fi connection"),
    UsbHint => ("接上 USB 傳輸線，並在手機允許 USB 偵錯。", "Connect your phone with a USB cable and allow USB debugging."),
    WifiHint => ("手機與電腦需連線到同一個 Wi-Fi", "Connect your phone and computer to the same Wi-Fi."),
    QrPath => ("開發人員選項 → 無線偵錯 → 使用 QR 圖碼配對裝置", "Developer options → Wireless debugging → Pair device with QR code"),
    Waiting => ("等待手機…", "Waiting for your phone…"),
    QrTitle => ("掃描，開始無線連線", "Scan to connect wirelessly"),
    QrEnded => ("掃描已結束，請重新配對。", "Scan ended. Please pair again."),
    Back => ("返回", "Back"),
    Rescan => ("重新掃描", "Rescan"),
    Repair => ("重新配對", "Pair again"),
    Phone => ("Android 手機", "Android phone"),
    Start => ("開啟 DeX", "Start DeX"),
    Stop => ("停止 DeX", "Stop DeX"),
    Idle => ("未連線", "Disconnected"),
    Connecting => ("連線中", "Connecting"),
    Projecting => ("投影中", "Projecting"),
    Error => ("錯誤", "Error"),
    Quality => ("畫質", "Quality"),
    High => ("高", "High"),
    Balanced => ("平衡", "Balanced"),
    Low => ("低", "Low"),
    Language => ("語言", "Language"),
    Chinese => ("繁體中文", "繁體中文"),
    English => ("English", "English"),
    Clear => ("清除連線設定", "Clear connection settings"),
    Show => ("顯示視窗", "Show window"),
    Quit => ("結束", "Quit"),
    Log => ("詳細紀錄", "Details and log"),
    Copy => ("複製", "Copy"),
    Check => ("重新檢查環境", "Check environment again"),
    Tooltip => ("Samsung DeX 視窗投影", "Samsung DeX desktop display"),
    NotFound => ("找不到手機的無線偵錯服務。請確認手機與電腦在同一 Wi-Fi，並開啟手機的無線偵錯；再重新掃描或重新配對。", "No wireless debugging service found. Use the same Wi-Fi and turn on Wireless debugging on your phone, then rescan or pair again."),
    Unauthorized => ("請解鎖手機並按「允許 USB 偵錯」。", "Unlock your phone and allow USB debugging."),
    Multiple => ("偵測到多台裝置，請只保留一台 USB 手機。", "Multiple devices found. Connect only one USB phone."),
    UsbTimeout => ("等待 USB 手機逾時，請檢查傳輸線與 USB 偵錯後返回重試。", "USB detection timed out. Check the cable and USB debugging, then go back and retry."),
    UsbPermissions => ("無法存取 USB 手機，請檢查 udev 權限與 USB 偵錯。", "Cannot access the USB phone. Check udev permissions and USB debugging."),
    ScanFailed => ("無法掃描 mDNS，請確認 adb 支援 mDNS 後重新掃描。", "Cannot scan mDNS. Check that adb supports mDNS, then rescan."),
    PairFailed => ("QR 配對失敗，請在手機重新開啟 QR 掃描並重新配對。", "QR pairing failed. Reopen the phone's QR scanner and pair again."),
    QrFailed => ("無法建立 QR 配對，請檢查系統後重新配對。", "Cannot create the pairing QR. Check your system and try again."),
    Cancelled => ("已停止掃描", "Scan cancelled"),
    Environment => ("缺少必要工具或版本不符，請安裝後重新檢查環境。", "Required tools are missing or outdated. Install them, then check the environment again."),
    NotReady => ("環境尚未就緒，請重新檢查環境並依安裝提示處理。", "Environment is not ready. Check it again and follow the installation hints."),
    ConfigFailed => ("無法讀寫設定，請檢查設定檔路徑、欄位與目錄權限。", "Cannot read or save settings. Check the config path, fields and directory permissions."),
    OperationFailed => ("操作失敗，請檢查手機連線與環境後重試。", "Operation failed. Check your phone connection and environment, then retry."),
    Network => ("手機與電腦可能位於不同網路，或手機 IP 已變更。請確認同一 Wi-Fi 並開啟無線偵錯，再重新掃描或重新配對。", "The phone and computer may be on different networks, or the phone's IP changed. Use the same Wi-Fi, enable Wireless debugging, then rescan or pair again."),
    ProjectionFailed => ("投影失敗，請檢查手機連線與 scrcpy（至少 3.2）後重試。", "Projection failed. Check your phone connection and scrcpy (3.2 or later), then retry."),
    DisconnectFailed => ("無法中斷電腦端連線，請檢查 adb 後重試清除設定。", "Cannot disconnect the computer-side connection. Check adb and retry clearing settings."),
    SaveFailed => ("無法儲存設定 {path}：{detail}；請檢查目錄與檔案權限後重試。", "Cannot save settings at {path}: {detail}. Check file and directory permissions, then retry."),
    ConfigUnreadable => ("設定檔無法讀取，請依視窗中的路徑與欄位提示修正後重新啟動。", "Cannot read the config. Correct the indicated path and fields, then restart."),
    MissingTools => ("缺少必要工具或版本不符，請安裝後按「重新檢查環境」：\n{detail}", "Required tools are missing or outdated. Install them, then check the environment again:\n{detail}"),
    ConnectFailed => ("無線連線失敗：{detail}；", "Wireless connection failed: {detail}. "),
    MetadataFailed => ("無法儲存手機資訊：{detail}；請檢查設定目錄權限。", "Cannot save phone information: {detail}. Check config directory permissions."),
    SessionExit => ("投影失敗（exit code {code}）。", "Projection failed (exit code {code}). "),
    ExitSuffix => (" (exit code {code})", " (exit code {code})"),
    SessionIo => ("投影失敗：{detail}。", "Projection failed: {detail}. "),
    ScrcpyHint => ("scrcpy 啟動或執行失敗；請確認版本至少為 3.2，並依下方訊息檢查手機連線後重試。", "scrcpy failed. Check that its version is at least 3.2 and follow the diagnostic below before retrying."),
    UsbRecovery => ("請檢查手機連線與環境，依下方訊息處理後重試。", "Check your phone connection and environment, follow the diagnostic below, then retry."),
    MonitorEnded => ("子行程監控已結束", "Command monitor ended"),
    MonitorFailed => ("無法啟動子行程監控", "Cannot start command monitor"),
    WindowEnded => ("視窗已結束", "Window closed"),
    CommandTimeout => ("指令逾時，請確認手機連線", "Command timed out. Check your phone connection."),
    NoAddress => ("未設定 wifi_address，請先連線", "No saved wireless address. Set up a connection first."),
    FontMissing => ("CJK font missing. Install google-noto-sans-cjk-fonts (Fedora) or fonts-noto-cjk (Debian/Ubuntu), then restart.", "CJK font missing. Install google-noto-sans-cjk-fonts (Fedora) or fonts-noto-cjk (Debian/Ubuntu), then restart."),
}

pub(super) const fn zh(key: Key) -> &'static str {
    get(Language::ZhTw, key)
}
pub(super) fn message(key: Key, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(zh(key).to_owned(), |text, (name, value)| {
            text.replace(name, value)
        })
}

pub(super) fn status(language: Language, status: super::Status) -> &'static str {
    get(
        language,
        match status {
            super::Status::Idle => Key::Idle,
            super::Status::Connecting => Key::Connecting,
            super::Status::Projecting => Key::Projecting,
            super::Status::Error => Key::Error,
        },
    )
}

// CLI/adb diagnostics remain original in the log. The inline card always provides
// a localized recovery instruction; raw external English lines retain useful details.
pub(super) fn diagnostic(language: Language, message: &str) -> String {
    for &(zh, en) in TABLE {
        if message == zh || message == en {
            return match language {
                Language::ZhTw => zh,
                Language::En => en,
            }
            .into();
        }
    }
    if language == Language::ZhTw {
        return message.into();
    }
    let key = if message.contains("尚未授權") || message.contains("unauthorized") {
        Key::Unauthorized
    } else if message.contains("Network is unreachable")
        || message.contains("No route to host")
        || message.contains("Wi-Fi")
    {
        Key::Network
    } else if message.contains("配對") {
        Key::PairFailed
    } else if message.contains("mDNS") {
        Key::ScanFailed
    } else if message.contains("設定") {
        Key::ConfigFailed
    } else if message.starts_with("投影失敗") {
        Key::ProjectionFailed
    } else if message.contains("環境") {
        Key::Environment
    } else {
        Key::ProjectionFailed
    };
    let mut result = get(language, key).to_owned();
    if let Some((reason, _)) = message
        .strip_prefix(zh(Key::SessionIo).split('{').next().unwrap_or_default())
        .and_then(|value| value.split_once('。'))
    {
        let reason = TABLE
            .iter()
            .find_map(|&(zh, en)| (reason == zh).then_some(en))
            .unwrap_or(reason);
        if reason.is_ascii() && !reason.is_empty() {
            result.push('\n');
            result.push_str(reason);
        }
    }
    if let Some(code) = message
        .split("exit code ")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .filter(|s| !s.is_empty())
    {
        result.push_str(&self::message(Key::ExitSuffix, &[("{code}", code)]));
    }
    for line in message
        .lines()
        .filter(|line| line.is_ascii() && !line.is_empty())
    {
        result.push('\n');
        result.push_str(line);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn english_projection_failure_is_not_reported_as_missing_tools() {
        assert_eq!(
            diagnostic(
                Language::En,
                "投影失敗（exit code 7）。請檢查手機連線與環境，依下方訊息處理後重試。"
            ),
            "Projection failed. Check your phone connection and scrcpy (3.2 or later), then retry. (exit code 7)"
        );
    }
    #[test]
    fn english_projection_failure_keeps_io_reason() {
        assert_eq!(
            diagnostic(
                Language::En,
                "投影失敗：Permission denied (os error 13)。請檢查手機連線與環境，依下方訊息處理後重試。"
            ),
            "Projection failed. Check your phone connection and scrcpy (3.2 or later), then retry.\nPermission denied (os error 13)"
        );
    }
    #[test]
    fn every_translation_has_both_languages() {
        assert!(
            TABLE
                .iter()
                .all(|(zh, en)| !zh.is_empty() && !en.is_empty())
        );
    }
    #[test]
    fn chinese_locale_defaults_to_traditional_chinese() {
        assert_eq!(
            Language::from_locale(None, Some("zh_CN.UTF-8")),
            Language::ZhTw
        );
    }
    #[test]
    fn non_chinese_locale_defaults_to_english() {
        assert_eq!(
            Language::from_locale(None, Some("en_US.UTF-8")),
            Language::En
        );
    }
    #[test]
    fn lc_all_overrides_lang() {
        assert_eq!(
            Language::from_locale(Some("en_GB"), Some("zh_TW")),
            Language::En
        );
    }
}
