# Changelog

## [Unreleased]

## [0.3.2] - 2026-11-04

### 修正

- Wi-Fi 投影連線無回應時會主動關閉凍結的 scrcpy 視窗，`--watch` 自動嘗試重連；未加 `--watch` 則回傳斷線狀態，不再無限等待。USB 行為不變。

- `start --wifi` 與無線 `--watch` 每次連線前先清除該位址的舊 adb transport，避免 Wi-Fi 中斷後誤報「already connected」而卡在讀取裝置資訊。清除失敗仍繼續連線，中斷則停止；手動 `dex2pc connect IP:PORT` 保留原行為，避免切斷其他使用中的工作階段。
- 無線連線輸出移除首尾空白，不再額外印出空白行。

- `dex2pc connect` 從 USB 切換時，先讀 Wi-Fi IP 再切換傳輸模式，避免手機重啟 adb 時出現 `error: closed`；切換後若連線尚未就緒，最多嘗試三次、間隔一秒。讀不到 IP 時不切換，指定 IP 連線與 `start --wifi` 行為不變。
- 無線配對失敗後顯示排除提示：重新開啟配對視窗並使用新的碼與埠、執行 `adb kill-server`、改用 Google 官方 platform-tools，或接 USB 執行 `dex2pc connect` 跳過配對。README 改為優先建議 USB 切換。

## [0.3.1] - 2026-11-03

### 新增

- **Wi-Fi 無線投影**（尚待實機驗收）：`dex2pc pair IP:PORT` 互動配對、`dex2pc connect IP:PORT` 連線並儲存位址，或 `dex2pc connect` 從 USB 切換。`dex2pc start --wifi [IP:PORT]` 使用指定或已儲存的位址開啟 DeX，`--watch` 在無線斷線後自動重連；`doctor` 顯示 Wi-Fi 連線，`config` 顯示 `wifi_address`。這是 scrcpy 的無線 adb 傳輸，不是原生 Wireless DeX／Miracast。

## [0.3.0] - 2026-10-11

提供 Fedora 的 rpm 與 Debian / Ubuntu 的 deb 安裝包，安裝包已由使用者測試確認沒有問題。

### 新增

- **Fedora 的 rpm 安裝包**（x86_64）：`sudo dnf copr enable zeno/scrcpy` 後 `sudo dnf install ./dex2pc-*.rpm` 就能裝好，會自動帶入 adb 與 scrcpy，並附上應用程式選單的捷徑，不必再跑 `dex2pc install-desktop`；移除用 `sudo dnf remove dex2pc`。也可以用 `packaging/build-rpm.sh` 從原始碼自己建
- **Debian 13 / Ubuntu 26.04 以上的 deb 安裝包**（amd64）：`sudo apt install ./dex2pc_*.deb`，會自動帶入 adb 並附上選單捷徑；移除用 `sudo apt remove dex2pc`。Ubuntu 26.04 的 scrcpy 夠新可以直接裝；Debian 13 沒有 scrcpy，README 說明怎麼先裝官方版 scrcpy。也可以用 `packaging/build-deb.sh` 從原始碼自己建
- 每次推送程式碼時，GitHub 都會自動建置 rpm 與 deb 並做安裝測試；只有 MINOR 以上的版本（`X.Y.0`）才會發佈 Release 並附上 SHA256 檢查碼，PATCH 版只留在 CI 的 artifact

### 修正

- `dex2pc doctor` / `start` 要求的 scrcpy 最低版本由 3.0 改為 **3.2**：DeX 視窗用到的 `--display-ime-policy` 從 3.2 才有，舊版會在檢查通過後才啟動失敗；現在會在檢查階段就提示更新

## [0.2.0] - 2026-10-10

第一個通過實機驗收的版本（Galaxy S23、One UI 8.5、scrcpy 5.0.1）：DeX 桌面、滑鼠、鍵盤（含中文）、剪貼簿、音訊、拔線重連、Ctrl+C 結束、小字清晰度都已確認。應用程式選單捷徑尚未驗收。

### 變更

- 已在實機（Galaxy S23、One UI 8.5）確認 `dex2pc start` 直接開出 DeX 桌面，**手機端不需要另外設定**；README 改寫為確認結果

### 修正

- DeX 視窗裡**鍵盤打不出字、剪貼簿貼不過去**：鍵盤預設改用 uhid 模式（設定檔 `keyboard` 仍可改回 `sdk`）
- DeX 視窗裡**右鍵沒有選單**（被當成「返回」）：右鍵與中鍵改為直接傳給手機，返回改用 Shift+右鍵
- 輸入法的選字窗改為顯示在 DeX 視窗裡，不跑到手機螢幕上
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
