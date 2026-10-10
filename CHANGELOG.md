# Changelog

## [Unreleased]

### 變更

- 已在實機（Galaxy S23、One UI 8.5）確認 `dex2pc start` 直接開出 DeX 桌面，**手機端不需要另外設定**；README 改寫為確認結果

### 修正

- `dex2pc install-desktop`：dex2pc 的路徑含換行或 tab 時也能建立有效的捷徑；含其他無法寫進捷徑的控制字元時直接報錯，不再產生損壞的捷徑

## [0.1.1] - 2026-10-09

第一個測試版，所有功能都**尚未在實機上驗收**。

### 新增

- `dex2pc` 指令列工具骨架：`--version`、`--help`
- `dex2pc doctor`：檢查 adb 與 scrcpy（3.0 以上）是否已安裝，缺少時印出 Fedora 的安裝指令
- `dex2pc doctor` 會檢查手機是否已連接：沒有裝置、未授權 USB 偵錯、沒有 USB 權限時說明怎麼處理；連了多台時用 `--serial` 指定
- `dex2pc doctor` 會列出手機型號、Android 與 One UI 版本，不是 One UI 8 以上的 Samsung 手機時說明原因
- `dex2pc doctor` 會顯示手機實際的 USB 連線速度，低於 USB 2.0 時提醒換線或改接主機板上的孔
- 設定檔 `~/.config/dex2pc/config.toml`（依 `XDG_CONFIG_HOME`）：可設定解析度、DPI、位元率、音訊、鍵鼠模式、視窗標題；`dex2pc config` 顯示實際生效的設定，`dex2pc config --path` 印出檔案位置
- CLI 參數可覆寫設定檔；音訊用 `--audio` / `--no-audio` 雙向切換，同時給時以最後一個為準
- `dex2pc start`：檢查環境後以 DeX 虛擬顯示模式啟動 scrcpy，預期 DeX 會開成電腦上的一個視窗（尚待實機驗證）；手機不支援時直接失敗，不啟動 scrcpy；scrcpy 出錯時印出它的錯誤訊息並沿用它的 exit code
- `dex2pc start --dry-run`：只印出會執行的 scrcpy 指令（可直接複製到終端機執行）
- `dex2pc start --watch`：USB 線拔除或斷線後，手機接回來會自動重新開啟 DeX 視窗；關閉視窗或按 Ctrl+C 結束，連續 5 次重新啟動失敗就放棄
- `dex2pc install-desktop`：在應用程式選單加入「Samsung DeX」捷徑，點它就開啟 DeX 視窗；`--uninstall` 移除
