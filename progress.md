# 進度與計畫：dex2pc rpm 安裝包

## 目標

讓 SamsungDex（dex2pc）可以用 rpm 安裝（`sudo dnf install ./dex2pc-*.rpm`）。

## 現況（2026-10-10）

> 以下三行是第一輪的觀察；現已更新，master 已併入，見「使用者回覆（第二輪）」。

- 這個 worktree（分支 `hb/18-samsungdex-rpm`）只有 Init commit，**沒有任何檔案**。
- 真正的程式在主 checkout `/home/Aquila/Documents/WorkSpace/SamsungDex2DesktopPC`：Rust crate `dex2pc` 0.2.0，
  有 Cargo.toml、src/、tests/、README、CHANGELOG，以及**已 staged 但未 commit** 的狀態（主 checkout 的 `feature_list.json` 是舊專案的計畫，不是這份）。
- 本機有 `rpmbuild`、cargo；沒有 `rpmlint`、`fpm`。
- 專案只支援 Fedora Linux；已有 `install-desktop` 子指令可建立使用者層級捷徑。

## 計畫

F1 spec 與本機 build 腳本 → F2 rpm 內容/相依/desktop 檔 → F3 Fedora 容器安裝煙霧測試
→ F8 deb 建置 → F9 Debian/Ubuntu 容器煙霧測試 → F4 CI 與發佈（rpm+deb）→ F5 rpm 文件 → F10 deb 文件 → F6 實機驗收。

## 使用者回覆（2026-10-10）

- 建置方式：**rpmbuild + 自寫 spec**（已確認）。
- 目標：**也要相容 RHEL/openSUSE 等其他 rpm 系**。影響：spec 只用通用巨集、套件相依不能寫死單一發行版的名稱、
  F3 煙霧測試改成多發行版 matrix、新增 F7（doctor 安裝提示依 os-release 調整）。
- 使用者問「改用 Flatpak 是否比較好、可避開相依問題」。結論：**不取代 rpm**。dex2pc 是呼叫主機上 adb 與 scrcpy 的 CLI，
  需要 USB 裝置存取與 adb server；Flatpak 沙箱得開 `--device=all`、自帶 adb 與 scrcpy、主機仍要 udev 規則，
  相依問題只是搬進套件包裡，且維護成本更高。使用者目標是 rpm，Flatpak 列為之後可選的獨立功能（見 questions）。
  跨發行版的相依問題改用 Recommends 與 doctor 的分發版提示處理。

## 使用者回覆（2026-10-10，第二輪）

- 發行版：**只做 Fedora**（推翻上一輪「相容 RHEL/openSUSE」）；但要**考慮 deb**。處理：打包檔分目錄（packaging/rpm/、packaging/common/），
  新增 F8 只寫 deb 評估筆記；是否真的做 deb 列在 questions。多發行版 matrix 與 F7（doctor 依發行版提示）已移除，**F7 的 id 不再使用**。
- scrcpy：**Requires 強制相依**。影響：Fedora 官方庫沒有 scrcpy，使用者要先 `dnf copr enable zeno/scrcpy`，否則 dnf 安裝失敗；
  F3 煙霧測試要先啟用 COPR（需要網路），README（F5）要寫明。
- doctor 提示維持只提示 Fedora 指令。
- 發佈：只放 GitHub Release。
- Flatpak：先不做，rpm 完成後再評估。
- repo 內容：主 checkout 現在有 `master` 分支，含完整程式（Cargo.toml 0.2.0 等）。已把 master 併進本分支（見下方），
  master 上舊專案的 feature_list.json / progress.md 衝突時以本計畫為準，舊內容留在 git 歷史。

## 使用者回覆（第三輪）

- deb：**現在就做**（Debian/Ubuntu）。F8 由「評估筆記」改為真正的 deb 建置，新增 F9（deb 容器煙霧測試）、F10（deb 文件）；
  F4 的 CI 與發佈同時涵蓋 rpm 與 deb，F6 實機驗收加上 Debian/Ubuntu。
- 建置方式：`dpkg-deb` 組出檔案樹（不新增 crate；與 rpm 一樣用平台原生工具），與 rpm 共用 `packaging/common/`。
- 仍只限 x86_64/amd64；`doctor` 提示維持只列 Fedora 指令，deb 使用者靠 README。
- 風險：Debian/Ubuntu 倉庫的 scrcpy 多半低於 3.2（dex2pc 目前要求 3.2 以上），相依與支援版本見 questions。

## 使用者回覆（第四輪）

- deb 支援 **Debian 13（trixie）與 Ubuntu 26.04 以上**（amd64）；倉庫 scrcpy 不夠新時，README 說明如何取得。
- deb 用 **`Depends: scrcpy (>= 3.2)` 強制相依**，與 rpm 一致。下限必須與程式內檢查的最低版本相同（目前 master 的程式要求 3.2，實作時再核對）。
- 影響：倉庫 scrcpy 太舊時 `apt install` 會失敗，所以 F9 在容器內用 equivs 建假的 scrcpy 3.2 套件滿足相依（要在輸出註明），
  並回報各映像檔實際的 scrcpy 版本，供 F10 寫 README。目前沒有未決問題。

## 使用者回覆（第五輪）

- GitHub 端要建置（我解讀為 CI，見 questions 確認）：每次 push/PR 都建置 rpm 與 deb；
  **只有「中版本」（MINOR，即 X.Y.0）以上才打 tag 並自動打包發佈**，PATCH 版不 tag、不發佈。
- **這覆蓋 coding-standards 的「每次進版打 tag `vX.Y.Z`」**：使用者本專案的明確指示優先，PATCH 版只更新 CHANGELOG、README badge，不打 tag。
  發佈 workflow 只接 `v*.*.0`。
- 目前以 **Public repo** 為主：用免費 GitHub-hosted runner 與內建 GITHUB_TOKEN，不設 secret；若日後改私有 repo 再評估（runner 額度、artifact 可見性）。

## 決策與理由

- `rpmbuild` + 自寫 spec（使用者已確認）。
- verify 沿用專案既有的 `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`；
  rpm 相關檢查寫成 cargo 整合測試（tests/rpm.rs），缺少 rpmbuild/rpmlint/容器時自動略過，CI 一定要跑。
- 版號只寫在 Cargo.toml，spec 由它讀取並用測試驗證一致；進版需同步 CHANGELOG、badge、tag `vX.Y.Z`。
- 套件相依：scrcpy 不在 Fedora 官方庫（需 COPR zeno/scrcpy），RHEL 系也沒有；使用者決定用 **Requires**。
- rpm 與 tarball 輸出到 `dist/`，不進 git。

## 給下一個 session

1. 只剩一個確認題（「CL」是不是 CI）；F1 到 F3 不受影響，可先開始；F9 查到的倉庫 scrcpy 版本要回寫到 F10 的 README，尤其是「repo 內容如何帶進這個分支」，否則 worktree 無法跑 verify。
2. 動手前載入 `coding-standards`；一個 commit 只做一件事；commit 格式 `<type>: <description>`。
3. 本機沒有 rpmlint，F2 需要時先請使用者安裝（`sudo dnf install rpmlint desktop-file-utils`）或在 CI 跑。
4. 實際發佈與實機驗收（F4 發佈、F6）需要使用者操作，不可假裝測過。
