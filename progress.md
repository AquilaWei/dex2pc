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

F1 spec 與本機 build 腳本 → F2 rpm 內容/相依/desktop 檔 → F3 Fedora 容器安裝煙霧測試
→ F4 GitHub Actions 建置與發佈（SHA256）→ F5 README/CHANGELOG → F6 實機驗收清單。

## 決策與理由

- 預設建議 `rpmbuild` + 自寫 spec：Fedora 原生、可上 COPR；待使用者確認（見 questions）。
- verify 沿用專案既有的 `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`；
  rpm 相關檢查寫成 cargo 整合測試（tests/rpm.rs），缺少 rpmbuild/rpmlint/容器時自動略過，CI 一定要跑。
- 版號只寫在 Cargo.toml，spec 由它讀取並用測試驗證一致；進版需同步 CHANGELOG、badge、tag `vX.Y.Z`。
- 套件相依：scrcpy 不在 Fedora 官方庫（需 COPR zeno/scrcpy），Requires 或 Recommends 待使用者決定。
- rpm 與 tarball 輸出到 `dist/`，不進 git。

## 給下一個 session

1. 先確認 questions 的答案，尤其是「repo 內容如何帶進這個分支」，否則 worktree 無法跑 verify。
2. 動手前載入 `coding-standards`；一個 commit 只做一件事；commit 格式 `<type>: <description>`。
3. 本機沒有 rpmlint，F2 需要時先請使用者安裝（`sudo dnf install rpmlint desktop-file-utils`）或在 CI 跑。
4. 實際發佈與實機驗收（F4 發佈、F6）需要使用者操作，不可假裝測過。
