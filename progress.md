# 進度與計畫：dex2pc rpm 安裝包

## 目標

讓 SamsungDex（dex2pc）可以用 rpm 安裝（`sudo dnf install ./dex2pc-*.rpm`）。

## 現況（2026-10-10）

- 這個 worktree（分支 `hb/18-samsungdex-rpm`）只有 Init commit，**沒有任何檔案**。
- 真正的程式在主 checkout `/home/Aquila/Documents/WorkSpace/SamsungDex2DesktopPC`：Rust crate `dex2pc` 0.2.0，
  有 Cargo.toml、src/、tests/、README、CHANGELOG，以及**已 staged 但未 commit** 的狀態（主 checkout 的 `feature_list.json` 是舊專案的計畫，不是這份）。
- 本機有 `rpmbuild`、cargo；沒有 `rpmlint`、`fpm`。
- 專案只支援 Fedora Linux；已有 `install-desktop` 子指令可建立使用者層級捷徑。

## 計畫

F1 spec 與本機 build 腳本 → F2 rpm 內容/相依/desktop 檔 → F3 多發行版容器安裝煙霧測試 → F7 doctor 依發行版提示
→ F4 GitHub Actions 建置與發佈（SHA256）→ F5 README/CHANGELOG → F6 實機驗收清單。

## 使用者回覆（2026-10-10）

- 建置方式：**rpmbuild + 自寫 spec**（已確認）。
- 目標：**也要相容 RHEL/openSUSE 等其他 rpm 系**。影響：spec 只用通用巨集、套件相依不能寫死單一發行版的名稱、
  F3 煙霧測試改成多發行版 matrix、新增 F7（doctor 安裝提示依 os-release 調整）。
- 使用者問「改用 Flatpak 是否比較好、可避開相依問題」。結論：**不取代 rpm**。dex2pc 是呼叫主機上 adb 與 scrcpy 的 CLI，
  需要 USB 裝置存取與 adb server；Flatpak 沙箱得開 `--device=all`、自帶 adb 與 scrcpy、主機仍要 udev 規則，
  相依問題只是搬進套件包裡，且維護成本更高。使用者目標是 rpm，Flatpak 列為之後可選的獨立功能（見 questions）。
  跨發行版的相依問題改用 Recommends 與 doctor 的分發版提示處理。

## 決策與理由

- `rpmbuild` + 自寫 spec（使用者已確認）。
- verify 沿用專案既有的 `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`；
  rpm 相關檢查寫成 cargo 整合測試（tests/rpm.rs），缺少 rpmbuild/rpmlint/容器時自動略過，CI 一定要跑。
- 版號只寫在 Cargo.toml，spec 由它讀取並用測試驗證一致；進版需同步 CHANGELOG、badge、tag `vX.Y.Z`。
- 套件相依：scrcpy 不在 Fedora 官方庫（需 COPR zeno/scrcpy），RHEL 系也沒有；Recommends（建議）或 Requires 待使用者決定。
- rpm 與 tarball 輸出到 `dist/`，不進 git。

## 給下一個 session

1. 先確認 questions 的答案（發行版範圍、scrcpy 相依、發佈位置、repo 內容、Flatpak），尤其是「repo 內容如何帶進這個分支」，否則 worktree 無法跑 verify。
2. 動手前載入 `coding-standards`；一個 commit 只做一件事；commit 格式 `<type>: <description>`。
3. 本機沒有 rpmlint，F2 需要時先請使用者安裝（`sudo dnf install rpmlint desktop-file-utils`）或在 CI 跑。
4. 實際發佈與實機驗收（F4 發佈、F6）需要使用者操作，不可假裝測過。
