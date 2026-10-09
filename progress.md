# 進度與計畫：SamsungDex2DesktopPC

## 目標

透過 USB-C 對 USB-C 線，把 Samsung DeX 以「一個程式視窗」的形式顯示在電腦上，並用電腦原本的鍵盤滑鼠直接操作。

## 調查結論（2026-10-09）

- **Samsung 官方的「DeX for PC」已停止支援**：One UI 7 起不再支援，Samsung 建議改用 Link to Windows，但那不是桌面模式。
- **USB-C 線的「資料」是雙向的，但「影像訊號」不能直接送進電腦**：手機接螢幕時用的是 DisplayPort Alt Mode，
  手機是訊號來源；筆電/桌機的 USB-C 孔也只會當 DisplayPort 來源（輸出），沒有接收端。兩個來源對接不會有畫面。
  所以做法是：手機把畫面壓成 H.264/H.265 影片串流，經 USB **資料**通道（ADB）傳到電腦解碼成視窗；
  鍵鼠事件再經同一條線反向傳回手機。這就是 scrcpy 的原理。線必須支援資料傳輸（有些線只能充電）。
- **可行路線：scrcpy 的虛擬顯示器**：scrcpy 3.0+ 的 `--new-display=WxH/DPI` 會在手機上建立一個虛擬顯示器，並把它開成電腦上的視窗，鍵鼠事件也會傳回手機。
  One UI 8（Android 16 Desktop Mode）起，DeX 改建在 Android 原生 Desktop Mode 上，可以跑在虛擬顯示器上，**不需要 root**。
  One UI 7 以前只能做一般鏡像（或要 HDMI dummy plug 之類的 hack）。
- 已有第三方包裝工具 DeXtop Mode（PyPI `dextop-mode`）做類似的事；計畫是自己寫一個 `dex2pc`，要不要改用現成工具列在 questions 裡等使用者決定。

## 頻寬估算（使用者問「頻寬夠嗎」，2026-10-09）

| 項目 | 需要 / 提供 |
|------|-------------|
| 不壓縮 1080p60（24-bit） | 約 3 Gbps（USB 2.0 不夠，這也是為什麼一定要壓縮） |
| 不壓縮 1440p60 | 約 5.3 Gbps |
| scrcpy 壓縮後影像（H.264/H.265） | 預設 8 Mbps，高畫質約 16–20 Mbps |
| 音訊（Opus） | 約 0.128 Mbps |
| 鍵鼠事件 | 幾 KB/s，可忽略 |
| USB 2.0 | 480 Mbps（實際約 300 Mbps 可用） |
| USB 3.2 Gen 1 | 5 Gbps |

結論：壓縮後的串流只用 USB 2.0 的一小部分，**理論上頻寬很充足**。實際限制在手機的硬體編碼器（解析度上限、發熱）、
編解碼延遲（通常幾十毫秒），以及壓縮造成的小字模糊（調高位元率可改善）。
因此新增 F11：doctor 讀 sysfs 顯示實際協商的 USB 速度，只在低於 USB 2.0（例如線材不良降到 12 Mbps）時警告。

## 使用者已確認（2026-10-09）

- 電腦平台：**只要 Linux（Fedora）**。安裝提示只寫 dnf；不做 Windows / macOS。
- 手機：**One UI 8 以上**。因此移除原本的「不支援時退回鏡像模式」：F4 不符合就報錯，F5 只組 DeX 指令，F7 不支援直接失敗。
- 尚未回答：要自己寫工具還是用現成工具、實作語言。計畫目前照建議（自己寫、Python + uv），若使用者改選，要重排計畫。

## 架構

Python 套件 `dex2pc`（uv 管理），本身不處理影像，只負責：

1. `doctor`：檢查 adb / scrcpy（>= 3.0）、裝置、One UI 版本
2. 組 scrcpy 指令（純函式，好測）
3. `start`：選裝置 → 檢查能力 → 以 DeX 虛擬顯示模式啟動 scrcpy
4. 選配：手機端 settings 套用與還原、斷線自動重連、Linux 桌面捷徑

所有和 adb / scrcpy 的互動都經過一個可注入的 runner，測試用假的 runner，不需要實機。

## 檢查指令

`uv run ruff check . && uv run ruff format --check . && uv run pytest -q`

在 F1 完成前這個指令一定失敗（還沒有 pyproject.toml），所以 F1 必須第一個做。

## 給下一個 session 的注意事項

- 目前 repo 是空的，只有 init commit。本機有 `uv`、Python 3.14，**沒有 adb、scrcpy**，
  所以測試全部要 mock，不能依賴這兩個工具存在。
- 依使用者規範（coding-standards skill）：commit 英文單行 `<type>: <desc>`；版號只寫在 `pyproject.toml`；
  功能與測試同一個 commit；CLAUDE.md、`.claude/` 不進 git。
- **實機才驗得出的項目**（不要假裝測過，列給使用者驗收）：DeX 是否真的出現在虛擬顯示器上、
  滑鼠點擊與捲動、鍵盤與中文輸入、剪貼簿、音訊、拔線重連。
- 不確定的事：
  - DeX 出現在虛擬顯示器上是否還需要額外的 adb settings 或手機上的開關（F8 就是為此保留的；網路上各教學說法不一）。
  - 預設 DPI：教學有用 160 也有用 284，F5 先用 1920x1080/160，等實機回饋再調。
  - 有教學說需要 scrcpy 4.x 以上；F2 先以 3.0 為下限，實機驗證後可能要調高。
  - Fedora 上 scrcpy 的建議安裝來源（COPR 或其他），F2 實作時查證，不要憑記憶寫。
- 使用者回答剩下的 questions 後可能要調整：語言、是否改用現成工具。

## 狀態

| 功能 | 狀態 |
|------|------|
| F1 專案骨架 | 未開始 |
| F2 doctor | 未開始 |
| F3 裝置偵測 | 未開始 |
| F4 能力判斷 | 未開始 |
| F11 USB 速度檢查（排在 F4 之後做） | 未開始 |
| F5 scrcpy 指令組裝 | 未開始 |
| F6 設定檔 | 未開始 |
| F7 start | 未開始 |
| F8 手機端設定 | 未開始 |
| F9 自動重連 | 未開始 |
| F10 桌面整合與交付 | 未開始 |
