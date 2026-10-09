use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

/// 低於 USB 2.0 High Speed（480 Mbps）通常代表線材不良或接在慢速 hub 上；
/// 壓縮後的串流只要約 8–20 Mbps，480 以上都夠用，所以只在這以下警告。
pub const MIN_MBPS: f64 = 480.0;

/// 在 `sysfs_root/bus/usb/devices/*` 中找 `serial` 檔內容等於 `serial` 的 USB 裝置，
/// 回傳它實際協商到的速度（Mbps，例如 480、5000；低速裝置是 1.5）。
///
/// `sysfs_root` 平常是 `/sys`，測試傳暫存目錄。找不到對應序號時回傳 `Ok(None)`，
/// 包括無線 adb（不經 USB）、或 sysfs 的 serial 和 adb 序號不同的手機；
/// 沒有 `bus/usb/devices` 目錄（例如沒有 USB 的環境）也算找不到。
///
/// # Errors
/// 列不出目錄（NotFound 以外的原因，例如權限），或找到裝置但 `speed` 讀不到、不是數字時回傳錯誤。
pub fn find_speed(sysfs_root: &Path, serial: &str) -> io::Result<Option<f64>> {
    let entries = match fs::read_dir(sysfs_root.join("bus/usb/devices")) {
        Ok(entries) => entries,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    for entry in entries {
        let dir = entry?.path();
        // interface 目錄（例如 3-2:1.0）和沒有序號的裝置沒有 serial 檔，跳過是正常的
        let Ok(found) = fs::read_to_string(dir.join("serial")) else {
            continue;
        };
        if found.trim() != serial {
            continue;
        }
        let speed = fs::read_to_string(dir.join("speed"))?;
        return speed.trim().parse().map(Some).map_err(|_| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("{} 不是數字：{}", dir.join("speed").display(), speed.trim()),
            )
        });
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    /// 在暫存的假 sysfs 下建立 `bus/usb/devices/<name>`，寫入給定的檔案。
    fn add_device(root: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = root.join("bus/usb/devices").join(name);
        fs::create_dir_all(&dir).unwrap();
        for (file, content) in files {
            fs::write(dir.join(file), content).unwrap();
        }
        dir
    }

    #[test]
    fn find_speed_with_matching_serial_returns_its_speed() {
        let root = TempDir::new().unwrap();
        add_device(
            root.path(),
            "3-2",
            &[("serial", "R5CW1234ABC\n"), ("speed", "5000\n")],
        );

        assert_eq!(
            find_speed(root.path(), "R5CW1234ABC").unwrap(),
            Some(5000.0)
        );
    }

    #[test]
    fn find_speed_skips_devices_with_other_serial() {
        let root = TempDir::new().unwrap();
        add_device(
            root.path(),
            "usb3",
            &[("serial", "0000:00:14.0\n"), ("speed", "5000\n")],
        );
        add_device(
            root.path(),
            "3-2",
            &[("serial", "R5CW1234ABC\n"), ("speed", "480\n")],
        );

        assert_eq!(find_speed(root.path(), "R5CW1234ABC").unwrap(), Some(480.0));
    }

    #[test]
    fn find_speed_skips_entries_without_serial_file() {
        let root = TempDir::new().unwrap();
        add_device(root.path(), "3-2:1.0", &[]);
        add_device(
            root.path(),
            "3-2",
            &[("serial", "R5CW1234ABC\n"), ("speed", "12\n")],
        );

        assert_eq!(find_speed(root.path(), "R5CW1234ABC").unwrap(), Some(12.0));
    }

    #[test]
    fn find_speed_reads_low_speed_with_decimal() {
        let root = TempDir::new().unwrap();
        add_device(
            root.path(),
            "3-2",
            &[("serial", "R5CW1234ABC\n"), ("speed", "1.5\n")],
        );

        assert_eq!(find_speed(root.path(), "R5CW1234ABC").unwrap(), Some(1.5));
    }

    #[test]
    fn find_speed_without_matching_serial_returns_none() {
        let root = TempDir::new().unwrap();
        add_device(
            root.path(),
            "3-2",
            &[("serial", "OTHER\n"), ("speed", "480\n")],
        );

        assert_eq!(find_speed(root.path(), "192.168.1.5:5555").unwrap(), None);
    }

    #[test]
    fn find_speed_without_usb_devices_dir_returns_none() {
        let root = TempDir::new().unwrap();

        assert_eq!(find_speed(root.path(), "R5CW1234ABC").unwrap(), None);
    }

    #[test]
    fn find_speed_with_non_numeric_speed_fails() {
        let root = TempDir::new().unwrap();
        add_device(
            root.path(),
            "3-2",
            &[("serial", "R5CW1234ABC\n"), ("speed", "fast\n")],
        );

        let err = find_speed(root.path(), "R5CW1234ABC").unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }
}
