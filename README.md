# dex2pc

![platform](https://img.shields.io/badge/platform-Fedora%20Linux-294172)

**用一條 USB-C 線，把 Samsung DeX 變成 Linux 電腦上的一個視窗，並直接用電腦的鍵盤滑鼠操作。**

> 目前狀態：**開發中**，`start` 可一鍵開啟 DeX 視窗，`doctor` 可檢查電腦端工具、手機連線、USB 速度與 One UI 版本，`config` 可查看設定；
> 手機端設定、自動重連、桌面捷徑還在實作，**尚未在實機上驗收**。
> 版號只寫在 `Cargo.toml`，用 `dex2pc --version`（或 `cargo run -- --version`）查看。

## 需求

- **電腦**：Fedora Linux（只支援 Linux）
- **手機**：Samsung，**One UI 8 以上**（DeX 改建在 Android 16 Desktop Mode 上，才能跑在虛擬顯示器）
- **adb**：`sudo dnf install android-tools`
- **scrcpy 3.0 以上**（需要 `--new-display`）：`sudo dnf copr enable zeno/scrcpy && sudo dnf install scrcpy`
- **支援資料傳輸的 USB-C 線**（有些線只能充電，接上後 `adb devices` 會看不到手機）
- 建置需要 Rust（cargo）

## 快速開始

1. 手機開啟 **USB 偵錯**：設定 → 關於手機 → 軟體資訊 → 連點「版本號碼」7 次開啟開發人員選項，再到 設定 → 開發人員選項 → 開啟「USB 偵錯」
2. 用**支援資料傳輸的 USB-C 線**接上電腦，手機跳出「允許 USB 偵錯？」時按**允許**
3. 執行：

```bash
cargo run --release -- start
```

（已用 `cargo install --path .` 安裝時直接執行 **`dex2pc start`**。）

`start` 會先做和 `doctor` 一樣的檢查，全部通過才開啟 **Samsung DeX** 視窗；直接用電腦的鍵盤滑鼠操作，**關閉視窗即結束**。
有問題時照畫面上的訊息處理，或先跑 `cargo run -- doctor` 看完整檢查結果。

```bash
cargo run -- start --dry-run               # 只印出會執行的 scrcpy 指令，不連手機
cargo run -- start --resolution 2560x1440 --video-bit-rate 16M   # 臨時覆寫設定檔
```

## 建置與執行

```bash
cargo build --release
cargo run -- --help
cargo run -- --version
cargo run -- doctor     # 檢查 adb、scrcpy 是否就緒，手機是否已連接並授權、USB 速度、是否為 One UI 8 以上
cargo run -- doctor --serial R5CW1234ABC   # 連了多台手機時指定其中一台
cargo run -- config     # 顯示設定檔位置與實際生效的設定
cargo run -- config --path   # 只印設定檔位置
cargo run -- config --audio  # 預覽覆寫後的設定（設定檔 audio = false 時重新開啟音訊）
```

## 設定檔

位置：**`$XDG_CONFIG_HOME/dex2pc/config.toml`**（沒設 `XDG_CONFIG_HOME` 時是 `~/.config/dex2pc/config.toml`）。
檔案不存在就用內建預設值；每個欄位都可省略，CLI 參數（例如 `--resolution 3840x2160`）會覆寫設定檔。
音訊可用 `--audio` / `--no-audio` 雙向覆寫設定檔的 `audio`；兩個都給時以**最後一個**為準。

```toml
resolution = "1920x1080"   # 虛擬顯示器解析度
dpi = 160                  # 數字越大介面越大
video_bit_rate = "16M"     # 省略時用 scrcpy 預設（8M）；小字模糊時調高
audio = true               # false 時不轉送手機音訊
keyboard = "uhid"          # sdk、uhid、aoa、disabled；省略時用 scrcpy 預設
mouse = "sdk"              # 同上
window_title = "Samsung DeX"
```

欄位名稱打錯或型別不對時，dex2pc 會指出檔案路徑與欄位並結束，不會默默忽略。

開發時的檢查指令：

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## 原理：為什麼不是直接走 DisplayPort？

- 手機接外接螢幕時走的是 USB-C 的 **DisplayPort Alt Mode**，手機是**訊號來源**。
  電腦的 USB-C 孔同樣只會**輸出**影像，沒有接收端，兩個來源對接不會有畫面。
- 所以畫面改走 USB 的**資料通道**：手機把畫面壓成 H.264/H.265 影片串流，經 ADB 傳到電腦解碼成視窗，
  鍵鼠事件再經同一條線傳回手機。壓縮後只需約 8–20 Mbps，USB 2.0 就足夠。
  `doctor` 會顯示手機實際協商到的 USB 速度，低於 480 Mbps（USB 2.0）時警告可能是線材或 hub 的問題。

### scrcpy 是什麼？

[scrcpy](https://github.com/Genymobile/scrcpy) 是 Genymobile 開發的**開源免費**工具（Apache 2.0）。
它經 ADB 在手機上暫時跑一個小程式，負責畫面串流、在電腦上開視窗、轉送鍵鼠事件；
**不需要在手機裝 App，也不需要 root**。scrcpy 3.0 起的 `--new-display` 能在手機上建立虛擬顯示器，DeX 就跑在上面。

dex2pc **不重寫**這些部分，只負責檢查環境、組好 scrcpy 參數、一鍵啟動。
