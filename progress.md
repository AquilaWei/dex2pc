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
- **scrcpy 是什麼**（使用者問過）：Genymobile 開發的開源免費工具（Apache 2.0），Linux / Windows / macOS 都能用。
  它經 ADB 在手機上暫時跑一個小程式，把畫面壓成影片串流傳到電腦、在電腦上開視窗顯示，並把電腦的鍵鼠事件送回手機；
  不需要在手機裝 App、不需要 root。畫面傳輸、解碼、鍵鼠轉送這些最難的部分都由它處理，dex2pc 不重寫。
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
- 實作語言：**Rust**。整份計畫已從 Python + uv 改為 cargo；功能範圍不變。
- 尚未回答：要做包裝工具還是只要教學（使用者回問「scrcpy 是什麼」，已在回覆中解釋並重新提問）。計畫目前照建議（包裝工具），若使用者改選，要重排計畫。

## 架構

Rust binary crate `dex2pc`（cargo），本身不處理影像，只負責：

1. `doctor`：檢查 adb / scrcpy（>= 3.0）、裝置、One UI 版本
2. 組 scrcpy 指令（純函式，好測）
3. `start`：選裝置 → 檢查能力 → 以 DeX 虛擬顯示模式啟動 scrcpy
4. 選配：手機端 settings 套用與還原、斷線自動重連、Linux 桌面捷徑

所有和 adb / scrcpy 的互動都經過一個 `CommandRunner` trait，測試用假的實作回傳寫死的輸出，不需要實機。
sysfs 根目錄、設定目錄、.desktop 目錄都以參數注入，測試不讀寫真實路徑、不改全域環境變數（Rust 測試預設平行執行）。

建議的 crate（實作時可依需要調整，新增依賴要有理由）：`clap`（derive）、`serde` + `toml`（設定檔）、
`tempfile`（dev-dependency，測試用暫存目錄）、F8/F9 需要攔 Ctrl+C 時再加 `ctrlc`。

## 檢查指令

`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

在 F1 完成前這個指令一定失敗（還沒有 Cargo.toml），所以 F1 必須第一個做。

## 給下一個 session 的注意事項

- F1 已完成：`Cargo.toml`（clap 4 derive、clippy pedantic 設為 warn）、`src/main.rs`、`tests/cli.rs`、README、CHANGELOG。
  `doctor` 子指令目前是佔位（印「尚未實作」並以 1 結束），讓 `--help` 有子指令區塊；F2 直接把它換成真的實作。
  `clippy.toml` 把 `DeX` 加進 doc-valid-idents，因為 doc comment 會變成 `--help` 文字，不能加反引號。
  CHANGELOG 目前只有 `[Unreleased]`，還沒打 tag（功能未實機驗收，照規範等出測試版時再進版）。
- F2 已完成：`src/runner.rs`（`CommandRunner` trait + `SystemRunner`；程式不存在回 `ErrorKind::NotFound`）、
  `src/doctor.rs`（`parse_scrcpy_version`、`parse_adb_version`、`run`）。doctor 兩項都檢查完才結束，一次列出所有問題。
  Fedora 的 scrcpy 安裝來源已查證：官方 `doc/linux.md` 寫 `dnf copr enable zeno/scrcpy && dnf install scrcpy`。
  F4/F11 要往 doctor 加裝置資訊時，在 `doctor::run` 裡接著加檢查即可；假 runner 在 `doctor.rs` 的 tests 模組。
  審查後補強：假 runner 只回應確切的 `adb version`、`scrcpy --version`，其他參數直接 panic，並記錄呼叫順序；
  回應可模擬成功、非 0 結束、找不到程式、權限錯誤。測試涵蓋 scrcpy 恰為 3.0 時通過，
  以及任一工具失敗時仍檢查另一個。已用改錯參數、改門檻、提早結束這幾種錯誤實作確認測試會失敗。
- F3 已完成：`src/device.rs`（`parse_devices` 解析 `adb devices -l`、`select` 依 `--serial` 選裝置，皆為純函式）。
  `doctor` 多了 `--serial/-s`，在 adb 可用時接著跑 `adb devices -l` 並印出「裝置：序號（型號）」；adb 不能用時不列裝置。
  選不到裝置的原因是 `SelectError`，`Display` 就是給使用者的訊息，F7 `start` 直接重用 `device::select`。
  除了 unauthorized，也處理 `no permissions`（Fedora 上 udev 權限不足常見）與 offline 等其他狀態。
  沒給 serial 時只算 `device` 狀態的裝置：一台 ready + 一台 unauthorized 會自動選 ready 那台。
  F4 要讀 getprop 時用 `adb -s <serial> shell getprop ...`，接在 `doctor::check_device` 成功之後。
- F4 已完成：`src/capability.rs`（`parse_one_ui`、`DeviceInfo::from_getprop`、`check`，皆為純函式）。
  `doctor::check_device` 改成回傳選到的序號，接著跑一次不帶參數的 `adb -s <serial> shell getprop`（一次讀全部屬性），
  印「型號：…（Android SDK …，One UI …）」與「判定：…」。製造商看 `ro.product.manufacturer`（不分大小寫）。
  Samsung 但讀不到/解析不了 oneui 也判定不支援。F7 `start` 直接重用 `capability::check`。
  getprop 輸出格式 `[key]: [value]` 與屬性名稱憑記憶寫，未在實機確認（需要實機驗收）。
- README 不寫版號：沒有 git remote，shields 的動態 badge 讀不到 Cargo.toml，所以拿掉版本 badge，改寫「用 `dex2pc --version` 查看」。
  F10 要求「README 版本 badge 與 Cargo.toml 一致」：repo 推上 GitHub 後改用讀 Cargo.toml 的動態 badge（`img.shields.io/badge/dynamic/toml`），不要寫死數字。
- 任務提示（harness）固定跑 `uv run ruff check . && uv run ruff format --check . && uv run pytest -q`，
  專案是 Rust，所以加了 `pyproject.toml`（只有 ruff、pytest 兩個 dev 依賴）與 `tests/test_cargo_verify.py`，
  pytest 會執行 `feature_list.json` 的 cargo verify 三個指令。兩個指令都要過；`.venv/` 已排除在 git 外。
- 原本 repo 只有計畫文件。本機有 Rust 1.98（cargo、clippy、rustfmt 皆可用），**沒有 adb、scrcpy**，
  所以測試全部要用假的 CommandRunner，不能依賴這兩個工具存在。
- 依使用者規範（coding-standards skill）：commit 英文單行 `<type>: <desc>`；版號只寫在 `Cargo.toml`；
  功能與測試同一個 commit；CLAUDE.md、`.claude/`、`target/` 不進 git；Cargo.lock 要提交（binary crate）。
- `features` 陣列順序是 F1–F4、F11、F5–F10，照陣列順序做。
- **實機才驗得出的項目**（不要假裝測過，列給使用者驗收）：DeX 是否真的出現在虛擬顯示器上、
  滑鼠點擊與捲動、鍵盤與中文輸入、剪貼簿、音訊、拔線重連。
- 不確定的事：
  - DeX 出現在虛擬顯示器上是否還需要額外的 adb settings 或手機上的開關（F8 就是為此保留的；網路上各教學說法不一）。
  - 預設 DPI：教學有用 160 也有用 284，F5 先用 1920x1080/160，等實機回饋再調。
  - 有教學說需要 scrcpy 4.x 以上；F2 先以 3.0 為下限，實機驗證後可能要調高。
  - Fedora 上 scrcpy 的建議安裝來源（COPR 或其他），F2 實作時查證，不要憑記憶寫。
- 使用者回答剩下的 question 後可能要調整：是否只要教學、不做工具。

## 狀態

| 功能 | 狀態 |
|------|------|
| F1 專案骨架 | 完成（2026-10-09） |
| F2 doctor | 完成（2026-10-09） |
| F3 裝置偵測 | 完成（2026-10-09） |
| F4 能力判斷 | 完成（2026-10-09） |
| F11 USB 速度檢查（排在 F4 之後做） | 未開始 |
| F5 scrcpy 指令組裝 | 未開始 |
| F6 設定檔 | 未開始 |
| F7 start | 未開始 |
| F8 手機端設定 | 未開始 |
| F9 自動重連 | 未開始 |
| F10 桌面整合與交付 | 未開始 |
