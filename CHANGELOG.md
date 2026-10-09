# Changelog

## [Unreleased]

### 新增

- `dex2pc` 指令列工具骨架：`--version`、`--help`
- `dex2pc doctor`：檢查 adb 與 scrcpy（3.0 以上）是否已安裝，缺少時印出 Fedora 的安裝指令
- `dex2pc doctor` 會檢查手機是否已連接：沒有裝置、未授權 USB 偵錯、沒有 USB 權限時說明怎麼處理；連了多台時用 `--serial` 指定
- `dex2pc doctor` 會列出手機型號、Android 與 One UI 版本，不是 One UI 8 以上的 Samsung 手機時說明原因
