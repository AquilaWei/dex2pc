use std::fmt;

/// `adb devices -l` 一列的連線狀態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// 已授權，可以下指令。
    Device,
    /// 手機上還沒按「允許 USB 偵錯」。
    Unauthorized,
    /// 電腦端沒有權限開啟 USB 裝置（adb 印 `no permissions (...)`）。
    NoPermissions,
    /// 其他狀態（`offline`、`recovery` 等），保留 adb 原字。
    Other(String),
}

/// `adb devices -l` 列出的一台裝置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    pub state: State,
    /// `model:` 欄位（例如 `SM_S918B`）；未授權的裝置通常沒有。
    pub model: Option<String>,
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.model {
            Some(model) => write!(f, "{}（{model}）", self.serial),
            None => write!(f, "{}", self.serial),
        }
    }
}

/// 解析 `adb devices -l` 的輸出。
///
/// 略過標題列、空行與 adb daemon 啟動時的 `* daemon ...` 訊息；無法辨識的列也略過。
pub fn parse_devices(output: &str) -> Vec<Device> {
    output.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<Device> {
    if line.starts_with("List of devices") || line.starts_with('*') {
        return None;
    }
    let mut words = line.split_whitespace();
    let serial = words.next()?.to_string();
    let state = match words.next()? {
        "device" => State::Device,
        "unauthorized" => State::Unauthorized,
        // `no permissions (...); see [...]` 會被空白拆開，只看第一個字
        "no" => State::NoPermissions,
        other => State::Other(other.to_string()),
    };
    let model = words
        .find_map(|word| word.strip_prefix("model:"))
        .map(str::to_string);
    Some(Device {
        serial,
        state,
        model,
    })
}

/// 選不出可用裝置的原因；`Display` 是給使用者看的完整說明。
#[derive(Debug, PartialEq, Eq)]
pub enum SelectError {
    NoDevice,
    Unauthorized(String),
    NoPermissions(String),
    NotReady {
        serial: String,
        state: String,
    },
    Multiple(Vec<Device>),
    SerialNotFound {
        serial: String,
        available: Vec<Device>,
    },
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => write!(
                f,
                "找不到裝置：請用支援資料傳輸的 USB-C 線連接手機並開啟 USB 偵錯（有些 USB-C 線只能充電）"
            ),
            Self::Unauthorized(serial) => {
                write!(f, "裝置 {serial} 尚未授權：請在手機上按「允許 USB 偵錯」")
            }
            Self::NoPermissions(serial) => write!(
                f,
                "電腦沒有權限存取裝置 {serial}（adb 回報 no permissions）：請確認 USB 裝置的 udev 權限設定"
            ),
            Self::NotReady { serial, state } => write!(
                f,
                "裝置 {serial} 目前狀態為 {state}，無法使用：請重新插拔 USB 線"
            ),
            Self::Multiple(devices) => {
                write!(f, "偵測到多台裝置，請用 --serial 指定其中一台：")?;
                write_list(f, devices)
            }
            Self::SerialNotFound { serial, available } if available.is_empty() => {
                write!(f, "找不到序號 {serial} 的裝置；目前沒有連接任何裝置")
            }
            Self::SerialNotFound { serial, available } => {
                write!(f, "找不到序號 {serial} 的裝置；目前連接的裝置：")?;
                write_list(f, available)
            }
        }
    }
}

fn write_list(f: &mut fmt::Formatter<'_>, devices: &[Device]) -> fmt::Result {
    for device in devices {
        write!(f, "\n  {device}")?;
    }
    Ok(())
}

impl std::error::Error for SelectError {}

/// 從 `devices` 挑出要用的裝置。
///
/// 有給 `serial` 時只看那一台；沒給時，恰好一台是 `device` 狀態就選它（其他未授權的不影響）。
///
/// # Errors
/// 沒有裝置、指定的序號不存在、多台可用卻沒指定，或選到的裝置不是 `device` 狀態時回傳對應的 [`SelectError`]。
pub fn select<'a>(devices: &'a [Device], serial: Option<&str>) -> Result<&'a Device, SelectError> {
    if let Some(serial) = serial {
        let device = devices.iter().find(|d| d.serial == serial).ok_or_else(|| {
            SelectError::SerialNotFound {
                serial: serial.to_string(),
                available: devices.to_vec(),
            }
        })?;
        return ready(device);
    }
    let usable: Vec<&Device> = devices
        .iter()
        .filter(|d| d.state == State::Device)
        .collect();
    match usable.as_slice() {
        [only] => Ok(only),
        [] => devices.first().map_or(Err(SelectError::NoDevice), ready),
        _ => Err(SelectError::Multiple(usable.into_iter().cloned().collect())),
    }
}

/// 裝置是 `device` 狀態就回傳它，否則把狀態轉成對應的錯誤。
fn ready(device: &Device) -> Result<&Device, SelectError> {
    let serial = device.serial.clone();
    match &device.state {
        State::Device => Ok(device),
        State::Unauthorized => Err(SelectError::Unauthorized(serial)),
        State::NoPermissions => Err(SelectError::NoPermissions(serial)),
        State::Other(state) => Err(SelectError::NotReady {
            serial,
            state: state.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: &str = "List of devices attached\n\n";
    const ONE: &str = "List of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\n\n";
    const UNAUTHORIZED: &str =
        "List of devices attached\nR5CW1234ABC            unauthorized usb:3-2 transport_id:1\n\n";
    const NO_PERMISSIONS: &str = "List of devices attached\nR5CW1234ABC            no permissions (missing udev rules? user is in the plugdev group); see [http://developer.android.com/tools/device.html] usb:3-2 transport_id:1\n\n";
    const OFFLINE: &str =
        "List of devices attached\nR5CW1234ABC            offline usb:3-2 transport_id:1\n\n";
    const TWO: &str = "List of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\nRFCX9876XYZ            device usb:3-4 product:gts9xxx model:SM_X710 device:gts9 transport_id:2\n\n";
    const ONE_READY_ONE_UNAUTHORIZED: &str = "List of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\nRFCX9876XYZ            unauthorized usb:3-4 transport_id:2\n\n";
    const DAEMON_STARTING: &str = "* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\nR5CW1234ABC            device usb:3-2 product:dm3qxxx model:SM_S918B device:dm3q transport_id:1\n\n";

    fn s918b() -> Device {
        Device {
            serial: "R5CW1234ABC".to_string(),
            state: State::Device,
            model: Some("SM_S918B".to_string()),
        }
    }

    fn x710() -> Device {
        Device {
            serial: "RFCX9876XYZ".to_string(),
            state: State::Device,
            model: Some("SM_X710".to_string()),
        }
    }

    #[test]
    fn parse_devices_with_header_only_returns_empty() {
        assert_eq!(parse_devices(NONE), vec![]);
    }

    #[test]
    fn parse_devices_reads_serial_state_and_model() {
        assert_eq!(parse_devices(ONE), vec![s918b()]);
    }

    #[test]
    fn parse_devices_reads_unauthorized_without_model() {
        assert_eq!(
            parse_devices(UNAUTHORIZED),
            vec![Device {
                serial: "R5CW1234ABC".to_string(),
                state: State::Unauthorized,
                model: None,
            }]
        );
    }

    #[test]
    fn parse_devices_reads_no_permissions_state() {
        assert_eq!(parse_devices(NO_PERMISSIONS)[0].state, State::NoPermissions);
    }

    #[test]
    fn parse_devices_keeps_unknown_state_text() {
        assert_eq!(
            parse_devices(OFFLINE)[0].state,
            State::Other("offline".to_string())
        );
    }

    #[test]
    fn parse_devices_skips_daemon_startup_messages() {
        assert_eq!(parse_devices(DAEMON_STARTING), vec![s918b()]);
    }

    #[test]
    fn select_without_devices_asks_for_data_cable_and_usb_debugging() {
        let err = select(&parse_devices(NONE), None).unwrap_err();

        assert_eq!(err, SelectError::NoDevice);
        assert_eq!(
            err.to_string(),
            "找不到裝置：請用支援資料傳輸的 USB-C 線連接手機並開啟 USB 偵錯（有些 USB-C 線只能充電）"
        );
    }

    #[test]
    fn select_unauthorized_device_asks_to_allow_usb_debugging() {
        let err = select(&parse_devices(UNAUTHORIZED), None).unwrap_err();

        assert_eq!(
            err.to_string(),
            "裝置 R5CW1234ABC 尚未授權：請在手機上按「允許 USB 偵錯」"
        );
    }

    #[test]
    fn select_no_permissions_device_points_to_udev() {
        let err = select(&parse_devices(NO_PERMISSIONS), None).unwrap_err();

        assert_eq!(err, SelectError::NoPermissions("R5CW1234ABC".to_string()));
    }

    #[test]
    fn select_offline_device_reports_its_state() {
        let err = select(&parse_devices(OFFLINE), None).unwrap_err();

        assert_eq!(
            err.to_string(),
            "裝置 R5CW1234ABC 目前狀態為 offline，無法使用：請重新插拔 USB 線"
        );
    }

    #[test]
    fn select_single_ready_device_picks_it() {
        let devices = parse_devices(ONE);

        assert_eq!(select(&devices, None), Ok(&s918b()));
    }

    #[test]
    fn select_one_ready_and_one_unauthorized_picks_the_ready_one() {
        let devices = parse_devices(ONE_READY_ONE_UNAUTHORIZED);

        assert_eq!(select(&devices, None), Ok(&s918b()));
    }

    #[test]
    fn select_multiple_ready_devices_without_serial_lists_serials() {
        let err = select(&parse_devices(TWO), None).unwrap_err();

        assert_eq!(err, SelectError::Multiple(vec![s918b(), x710()]));
        assert_eq!(
            err.to_string(),
            "偵測到多台裝置，請用 --serial 指定其中一台：\n  R5CW1234ABC（SM_S918B）\n  RFCX9876XYZ（SM_X710）"
        );
    }

    #[test]
    fn select_with_serial_picks_that_device_among_many() {
        let devices = parse_devices(TWO);

        assert_eq!(select(&devices, Some("RFCX9876XYZ")), Ok(&x710()));
    }

    #[test]
    fn select_with_unknown_serial_reports_it_and_lists_connected_devices() {
        let err = select(&parse_devices(TWO), Some("NOPE123")).unwrap_err();

        assert_eq!(
            err.to_string(),
            "找不到序號 NOPE123 的裝置；目前連接的裝置：\n  R5CW1234ABC（SM_S918B）\n  RFCX9876XYZ（SM_X710）"
        );
    }

    #[test]
    fn select_with_serial_when_nothing_connected_says_no_devices() {
        let err = select(&parse_devices(NONE), Some("NOPE123")).unwrap_err();

        assert_eq!(
            err.to_string(),
            "找不到序號 NOPE123 的裝置；目前沒有連接任何裝置"
        );
    }

    #[test]
    fn select_with_serial_of_unauthorized_device_asks_to_allow_usb_debugging() {
        let err = select(
            &parse_devices(ONE_READY_ONE_UNAUTHORIZED),
            Some("RFCX9876XYZ"),
        )
        .unwrap_err();

        assert_eq!(err, SelectError::Unauthorized("RFCX9876XYZ".to_string()));
    }
}
