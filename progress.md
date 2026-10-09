# 進度與計畫：SamsungDex2DesktopPC

## 目標

透過 USB-C 對 USB-C 線，把 Samsung DeX 以「一個程式視窗」的形式顯示在電腦上，並用電腦原本的鍵盤滑鼠直接操作。

## 調查結論（2026-10-09）

- **Samsung 官方的「DeX for PC」已停止支援**：One UI 7 起不再支援，Samsung 建議改用 Link to Windows，但那不是桌面模式。
- **USB-C 線不能把影像「送進」電腦**：電腦的 USB-C 是 DisplayPort 輸出，不是輸入；所以影像一定要靠軟體經 USB 資料通道（ADB）傳過來。
- **可行路線：scrcpy 的虛擬顯示器**：scrcpy 3.0+ 的 `--new-display=WxH/DPI` 會在手機上建立一個虛擬顯示器，並把它開成電腦上的視窗，鍵鼠事件也會傳回手機。
  One UI 8（Android 16 Desktop Mode）起，DeX 改建在 Android 原生 Desktop Mode 上，可以跑在虛擬顯示器上，**不需要 root**。
  One UI 7 以前只能做一般鏡像（或要 HDMI dummy plug 之類的 hack）。
- 已有第三方包裝工具 DeXtop Mode（PyPI `dextop-mode`）做類似的事；計畫是自己寫一個 `dex2pc`，要不要改用現成工具列在 questions 裡等使用者決定。

## 架構

Python 套件 `dex2pc`（uv 管理），本身不處理影像，只負責：

1. `doctor`：檢查 adb / scrcpy（>= 3.0）、裝置、One UI 版本
2. 組 scrcpy 指令（純函式，好測）
3. `start`：選裝置 → 判斷能力 → 啟動 scrcpy（DeX 模式或退回鏡像）
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
- 使用者回答 questions 後可能要調整：平台範圍（影響 F2 安裝提示、F10 桌面捷徑）、語言、是否改用現成工具。

## 狀態

| 功能 | 狀態 |
|------|------|
| F1 專案骨架 | 未開始 |
| F2 doctor | 未開始 |
| F3 裝置偵測 | 未開始 |
| F4 能力判斷 | 未開始 |
| F5 scrcpy 指令組裝 | 未開始 |
| F6 設定檔 | 未開始 |
| F7 start | 未開始 |
| F8 手機端設定 | 未開始 |
| F9 自動重連 | 未開始 |
| F10 桌面整合與交付 | 未開始 |
