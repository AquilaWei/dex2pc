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

- **所有功能都已完成**（F8 於 2026-10-10 實機確認後標 true）。
- **2026-10-10 實機驗證**（使用者授權手機並安裝 scrcpy 5.0.1 後）：
  - 手機：Galaxy S23（`SM-S9180`）、Android 16（SDK 36）、`ro.build.version.oneui` = `80500`（One UI 8.5）。
  - `dex2pc doctor` 全部通過，輸出與預期一致：getprop 格式、One UI 解析、sysfs USB 速度（這次接在 USB 3 孔，5000 Mbps）都正確。
  - `dex2pc start` 以 `--new-display=1920x1080/160` 建立虛擬顯示器（`dumpsys display` 的 displayId 7），上面直接跑
    `com.sec.android.app.launcher/com.honeyspace.dexservice.SecondaryLauncher`（DeX 桌面），**沒有改任何手機設定**。
    用 `adb exec-out screencap -d <SurfaceFlinger 的 display id> -p` 截圖確認有 DeX 的桌面圖示與工作列
    （`screencap -d` 要用 `dumpsys SurfaceFlinger --display-id` 列出的 64 位元 id，不是 displayId 7）。
  - scrcpy 被 `timeout` 結束後，虛擬顯示器自動消失。
  - 因此 F8 走步驟 1 的「完全不需要」分支：只在 README 記錄結論，不寫套用/還原設定的程式，`passes` 標 true。
  - 仍未驗：滑鼠鍵盤（含中文輸入）、剪貼簿、音訊、拔線重連、Ctrl+C、桌面捷徑。這些要使用者在電腦前操作，列在 README 實機驗收清單。
- 2026-10-09 session：本機接著一台 Samsung 手機（lsusb `04e8:6864`，USB 偵錯已開），系統 PATH 沒有 adb，
  但 Android SDK 有 `~/Android/Sdk/platform-tools/adb`；**沒有 scrcpy**。`adb devices -l` 顯示 `unauthorized usb:3-1`，
  要使用者在手機上按「允許 USB 偵錯」才能讀 getprop 或跑 scrcpy，非互動 session 做不到，所以 F8 仍維持 false。
  只跑了唯讀指令，結束前已 `adb kill-server`。使用者授權並安裝 scrcpy（`dnf copr enable zeno/scrcpy && dnf install scrcpy`）後，
  下一個 session 可用 `PATH=~/Android/Sdk/platform-tools:$PATH dex2pc doctor` 先驗 F2–F4、F11，再由使用者看 `dex2pc start` 是否出現 DeX。
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
- F11 已完成：`src/usb.rs`（`find_speed`、`MIN_MBPS`）。`doctor::run` 多一個 `sysfs_root` 參數（main 傳 `/sys`），
  選到裝置後、讀 getprop 之前印「連線：USB N Mbps」；低於 480 多印一行警告。USB 速度只是診斷，不影響 doctor 結果。
  速度以 f64 解析，因為低速裝置的 sysfs speed 是 `1.5`。找不到序號或沒有 `bus/usb/devices` 目錄印「無法判斷 USB 速度」；
  列目錄權限錯誤或 speed 不是數字時印「無法判斷 USB 速度（原因）」，同樣不視為錯誤。
  doctor 測試的預設假 sysfs 是空暫存目錄，所以既有的完整輸出測試多了「連線：無法判斷 USB 速度」一行。
  sysfs 的 serial 等於 adb 序號：2026-10-09 實機確認（`/sys/bus/usb/devices/3-1/serial` 與 `adb devices -l` 都是同一個序號，speed 為 480）。
- F5 已完成：`src/scrcpy.rs`（`Resolution`、`Dpi` 實作 `FromStr`，內部用 `NonZeroU32`；`Options` + `Default`；`build_args`）。
  不合法的解析度/DPI 在解析時就回傳 `InvalidDisplay`，所以 `build_args` 不會失敗；F6 讀設定檔、clap 解析 `--resolution` 都直接用 `FromStr`。
  `build_args` 回傳不含 `scrcpy` 程式名稱的參數，順序固定：serial → new-display → window-title → bit-rate → no-audio → keyboard → mouse。
  鍵鼠模式與位元率是 `Option`，`None` 時不加參數，沿用 scrcpy 預設；位元率字串原樣傳給 scrcpy，由它檢查格式。
  `main.rs` 對 `mod scrcpy` 加了 `#[cfg_attr(not(test), expect(dead_code))]` 與 TODO，F7 接上後要移除（否則 expect 不成立會報 warning）。
  F5 尚無使用者看得到的效果，CHANGELOG 沒有新增項目。
- F6 已完成：`src/config.rs`（`path`、`load`、`Overrides`、`show`）與 `dex2pc config [--path] [覆寫參數]`。
  設定檔先解析成 `toml::Table` 再逐欄位轉成 `scrcpy::Options`（沒用 serde derive），這樣每個錯誤都能明確寫出欄位名稱；
  欄位：resolution（字串）、dpi（整數）、video_bit_rate（字串）、audio（bool）、keyboard/mouse（字串）、window_title（字串）。
  `load` 檔案不存在回 `Ok(None)`，呼叫端用 `unwrap_or_default()`。空字串或相對路徑的 `XDG_CONFIG_HOME` 依 XDG 規範視同未設定。
  `config::Overrides` 是 clap `Args`，F7 的 `start` 直接 `#[command(flatten)]` 它，再 `overrides.apply(config::load(..)?.unwrap_or_default())`。
  `--no-audio` 只能關音訊，不能把設定檔的 `audio = false` 打開（沒有 `--audio`）。
  `InputMode` 多了 `FromStr`/`Display`，設定檔與 CLI 共用同一個錯誤訊息。
  `expect(dead_code)` 從整個 `mod scrcpy` 縮到 `build_args` 一個函式，F7 接上後移除。
  clippy.toml 的 doc-valid-idents 加了 `XDG_CONFIG_HOME`（`--help` 文字不能有反引號）。
  CLI 整合測試只在子行程設定 `XDG_CONFIG_HOME`，不動測試行程的環境變數。
- F7 已完成：`src/start.rs`（`run`、`dry_run`）與 `dex2pc start [--serial] [--dry-run] [覆寫參數]`。
  `doctor::check` 是新的共用入口：檢查內容和輸出與 `doctor::run` 相同，全部就緒時回傳選到的序號；`run` 改成呼叫它。
  `start` 一律帶 `--serial=<選到的序號>` 給 scrcpy，避免連多台時 scrcpy 選錯。
  `--dry-run` 不經過 runner，結構上就不可能執行外部指令，也因此不選裝置（沒給 `--serial` 時指令裡沒有 serial）；
  印出的指令以單引號處理含空白的參數，可直接貼到 shell。
  scrcpy 經現有的 `CommandRunner::run`（`.output()`，**輸出被收集**）執行：好處是失敗時能把它的 stderr 印出來、F9 也能看
  stderr 判斷是不是斷線；代價是 scrcpy 執行中的 INFO 訊息看不到，正常關閉時不印。若實機覺得需要即時 log，再考慮加一個繼承 stdio 的方法。
  exit code：scrcpy 0 → 0；非 0 → 相同 code 並印「scrcpy 以 exit code N 結束：」與 stderr（stderr 空時改印 stdout）；
  被訊號中止或無法執行 → 1；環境檢查不通過 → 1 且不啟動。Ctrl+C 時 SIGINT 同時送到 dex2pc 與 scrcpy，沒有特別處理（F8 要還原設定時才需要攔）。
  `scrcpy::build_args` 上的 `expect(dead_code)` 已移除。
- F8 調查（2026-10-09，**未在實機確認，`passes` 維持 false**）：查了 Android Authority、PhoneArena、
  uzair.is-a.dev（One UI 8 DeX 教學）、dev.to（Mac 上跑 DeX）等社群教學，**沒有任何一篇用到 `adb shell settings put`**；
  都只開 USB 偵錯後執行 `scrcpy --new-display=…`。網路上找不到 `force_desktop_mode_on_external_displays`、
  `enable_freeform_support` 對 One UI 8 DeX 有作用的來源。因此照 F8 步驟 1 的分支，暫定「不需要」，只在 README 記錄結論，
  **沒有寫套用/還原設定的程式**（不為推測中的需求寫程式）。README 請使用者在 DeX 沒出現時回報 `settings list global` 的相關項目。
  uzair 的教學另外用 `adb shell wm set-minimum-width 411` 修手機本身螢幕模糊；它不影響 DeX 視窗、沒寫怎麼還原，
  所以不放進 README、也不自動執行。
  README 依審查意見寫成**待實機確認的假設**，不寫成確定行為：scrcpy 官方 `doc/virtual-display.md` 只保證建立虛擬顯示器，
  且說明沒有 launcher 的裝置畫面會是空白，沒有保證會啟動 DeX。
  **F8 要標 true 需要使用者在實機上確認**：`dex2pc start` 是否直接出現 DeX。若出現 → 標 true，結束 F8；
  若沒出現 → 依使用者回報的設定實作步驟 2–5（套用前讀原值、null 還原用 `settings delete`、`--keep-settings`、攔 Ctrl+C）。
  使用者還沒回報前，下一個 session 不要重做 F8 的調查，直接做 F9（F9 不依賴 F8）。
- F9 已完成：`start::run` 多一個 `watch: Option<&Watch>`（`max_failures` 預設 5、`retry_delay` 預設 2 秒，測試用 0）。
  斷線判斷依 scrcpy man page 的 Exit status（已查證）：0 正常結束、1 初次連線失敗、**2 連線中途斷開**；只有 2 會重連。
  重連流程：`adb -s <serial> wait-for-device`（阻塞到手機回來）→ 等 `retry_delay` → 用同一組參數重新啟動 scrcpy，不重跑整個 doctor。
  「連續失敗」= 斷線後重新啟動失敗（scrcpy 非 0/2、無法執行，或 wait-for-device 失敗）；scrcpy 再次回 2 代表有連上過，計數歸零。
  審查後修正：每次失敗（含 wait-for-device 失敗，它會立刻返回）後也先等 `retry_delay` 再試，避免暫時性 adb 故障瞬間耗盡 5 次；
  第 5 次失敗直接放棄不再等。等待函式以參數注入 `keep_reconnecting`，回歸測試記錄等待時間而不真的等。
  第一次啟動就失敗不重試（多半是設定/環境問題）。被訊號中止（scrcpy 或 adb 的 code 為 None）視為 Ctrl+C，直接結束、回 1。
  Ctrl+C 沒有攔：SIGINT 同時送到 dex2pc、scrcpy、adb，dex2pc 依預設行為結束；目前沒有要還原的狀態，所以不需要 `ctrlc`（F8 若要還原設定再加）。
  缺點：同一手機若「連上後立刻斷線」不斷重複，計數一直歸零，會一直重試（每次間隔 `retry_delay`），使用者需 Ctrl+C。
  `--watch` 搭配 `--dry-run` 時 `--watch` 無作用。實機待驗：拔線後 scrcpy 真的回 2、接回後 `wait-for-device` 能等到、2 秒是否夠手機就緒。
- F10 已完成：`src/desktop.rs`（`applications_dir`、`entry`、`install`、`uninstall`）與 `dex2pc install-desktop [--uninstall]`。
  `Exec` 寫 `env::current_exe()` 的**絕對路徑** + ` start`，因為桌面環境的 PATH 常不含 `~/.cargo/bin`；
  代價是用 `cargo run` 建立時會指向 `target/`，README 已說明要在 `cargo install` 之後執行。
  路徑一律加雙引號，依 Desktop Entry 規範跳脫 `"` `` ` `` `$` `\`（string 層再跳脫一次反斜線）與 `%`→`%%`。
  審查後補上：換行、tab、CR 用 string 層的 `\n` `\t` `\r` 表示（否則換行會把 `.desktop` 切壞）；
  其他控制字元規範沒有表示法，`entry` 回傳 `ControlCharInPath` 拒絕安裝。已用 `desktop-file-validate` 與 GLib
  （`GKeyFile` + `g_shell_parse_argv`）確認含換行、tab 的路徑能還原成原本的參數。
  不加 `--watch`：捷徑 `Terminal=false`，`wait-for-device` 等不到手機時會在背景一直等、使用者看不到。
  產生的檔案在本機用 `desktop-file-validate` 驗證通過；`Icon=video-display` 是 freedesktop 標準圖示名稱。
  空字串/相對路徑的 `XDG_DATA_HOME` 和 `XDG_CONFIG_HOME` 一樣視同未設定；clippy.toml 的 doc-valid-idents 加了 `XDG_DATA_HOME`。
  README 新增「安裝」「實機驗收清單」「頻寬」「運作方式」（Mermaid 連線／操作流程圖，已用 mermaid-cli 確認能渲染）幾節。
  版本 badge 由 **`cargo run --example sync_version_badge`** 從 `CARGO_PKG_VERSION` 產生，不手改；
  `tests/docs.rs` 透過 `#[path]` 共用同一份 `sync_badge`，README 不是最新產物時失敗並提示指令（已用改舊 badge 確認會失敗）；
  CHANGELOG 仍檢查有 `## [<版號>]` 段落。
  版號 0.1.0 → **0.1.1**（功能未實機驗收，照規範出 PATCH 測試版；0.1.0 從沒打過 tag），單獨一個 commit，打了本機 tag `v0.1.1`（未推送，沒有 remote）。
- README 版本 badge：沒有 git remote，shields 的動態 badge 讀不到 Cargo.toml，所以改由 example 產生。
  **進版流程**：改 `Cargo.toml` → `cargo run --example sync_version_badge` → 更新 CHANGELOG → 測試 → commit → tag。
  推上 GitHub 後若改用讀 Cargo.toml 的動態 badge，要一起刪掉 example 與 `tests/docs.rs` 的 badge 測試。
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
  - ~~DeX 出現在虛擬顯示器上是否還需要額外的 adb settings~~：2026-10-10 實機確認不需要（One UI 8.5）。
  - 預設 DPI：教學有用 160 也有用 284，F5 先用 1920x1080/160，等實機回饋再調。
  - 有教學說需要 scrcpy 4.x 以上；F2 先以 3.0 為下限，實機驗證後可能要調高。
  - Fedora 上 scrcpy 的建議安裝來源（COPR 或其他），F2 實作時查證，不要憑記憶寫。
- 使用者回答剩下的 question 後可能要調整：是否只要教學、不做工具。

- **2026-10-10 實機驗收（使用者）**：音訊正常；滑鼠右鍵、鍵盤、剪貼簿原本失效。
  依 `scrcpy --help`：sdk 滑鼠預設把右鍵綁成返回（`bhsn`）。改用
  `--keyboard=uhid --mouse-bind=++++:bhsn --display-ime-policy=local` 後使用者確認右鍵、打字、剪貼簿都正常，
  所以 `Options::default` 鍵盤改 uhid，`build_args` 一律加 mouse-bind 與 IME policy。
  先前回報的「拔線重連失敗」是在沒加 `--watch` 的 scrcpy 視窗上拔線，結束是預期行為；
  之後用 `dex2pc start --watch` 實測，log 顯示 scrcpy 以 exit code 2 結束 → 等待 → 重新啟動，使用者確認視窗自動回來。
  **待確認**：中文輸入、快捷鍵、Ctrl+C、桌面捷徑。

## 狀態

| 功能 | 狀態 |
|------|------|
| F1 專案骨架 | 完成（2026-10-09） |
| F2 doctor | 完成（2026-10-09） |
| F3 裝置偵測 | 完成（2026-10-09） |
| F4 能力判斷 | 完成（2026-10-09） |
| F11 USB 速度檢查（排在 F4 之後做） | 完成（2026-10-09） |
| F5 scrcpy 指令組裝 | 完成（2026-10-09） |
| F6 設定檔 | 完成（2026-10-09；審查後補 `--audio` 雙向覆寫，與 `--no-audio` 同時給時最後一個為準） |
| F7 start | 完成（2026-10-09；實機未驗收） |
| F8 手機端設定 | 完成（2026-10-10；實機確認不需要設定，只在 README 記錄結論） |
| F9 自動重連 | 完成（2026-10-09；實機未驗收） |
| F10 桌面整合與交付 | 完成（2026-10-09；v0.1.1 測試版，實機未驗收） |
