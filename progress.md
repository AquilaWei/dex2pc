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

F11 共用基礎 → （rpm 軌道與 deb 軌道平行，見下方「平行執行」）F1 spec 與本機 build 腳本 → F2 rpm 內容/相依/desktop 檔 → F3 Fedora 容器安裝煙霧測試
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

- GitHub 端要建置（已確認是 CI，不是 CHANGELOG）：每次 push/PR 都建置 rpm 與 deb；
  **只有「中版本」（MINOR，即 X.Y.0）以上才打 tag 並自動打包發佈**，PATCH 版不 tag、不發佈。
- **這覆蓋 coding-standards 的「每次進版打 tag `vX.Y.Z`」**：使用者本專案的明確指示優先，PATCH 版只更新 CHANGELOG、README badge，不打 tag。
  發佈 workflow 只接 `v*.*.0`。
- 目前以 **Public repo** 為主：用免費 GitHub-hosted runner 與內建 GITHUB_TOKEN，不設 secret；若日後改私有 repo 再評估（runner 額度、artifact 可見性）。

## 平行執行（第六輪，使用者要求：能拆成 subagent 平行就盡量平行）

**結論：可以，分成兩條獨立的軌道，前面一個小的共用基礎、後面一個匯合點。**

| 階段 | 功能 | 依賴 | 備註 |
|------|------|------|------|
| 0 | F11 打包共用基礎 | 無 | 先做完（小）：共用 .desktop、version.sh、.gitignore。這是 rpm 與 deb 唯一共用的檔案，先做才不會互相衝突 |
| 1 | rpm 軌道 F1 → F2 → F3 → F5 | F11 | 檔案範圍：`packaging/rpm/`、`packaging/build-rpm.sh`、`packaging/smoke-test.sh`、`tests/rpm.rs` |
| 1 | deb 軌道 F8 → F9 → F10 | F11 | 檔案範圍：`packaging/deb/`、`packaging/build-deb.sh`、`packaging/smoke-test-deb.sh`、`tests/deb.rs` |
| 2 | F4 CI 與發佈 | F3、F9 | 需要兩條軌道的腳本都存在；`.github/workflows/` 是它獨佔的目錄 |
| 3 | F6 實機驗收清單 | F4、F5、F10 | 使用者操作 |

- 兩條軌道的程式、腳本、測試**檔案互不重疊**，可各由一個 subagent 在**各自的 git worktree**（`isolation: "worktree"`）同時做；
  同一軌道內的功能依序做（後者用前者的產物）。
- **會衝突的共用檔案只有 README.md 與 CHANGELOG.md**（F5 與 F10 都要改）。規則：F5 只動 README 的 rpm 段落，F10 只動 deb 段落，
  CHANGELOG 各自在 [Unreleased] 加一行；合併時由主 session 解衝突，不叫 subagent 自己合併。
- 合併順序：F11 → 兩條軌道各自 commit 後由主 session 依序併回 → 跑完整 verify → 再做 F4。每個 subagent 只在自己的 worktree 跑 verify，
  功能是否完成以併回後的完整 verify 為準。
- 同一軌道內能再拆的：F3、F9 的容器腳本與測試可與 F5、F10 的文件平行（文件只依賴 F2、F8 已決定的內容）。
  但文件段落要等 F3、F9 查到真實資料（COPR、倉庫 scrcpy 版本）才能定稿，所以建議先平行起草、最後再回填。
- 不適合平行的：F4（要等兩條軌道的腳本）、F6（需人工）、F11（是其他功能的前置）。
- 沒有在 feature_list.json 增加額外欄位（避免破壞既有解析），依賴關係寫在這張表。
- 每個 subagent 動手前都要載入 `coding-standards`，並遵守「一個 commit 只做一件事」。

## 決策與理由

- `rpmbuild` + 自寫 spec（使用者已確認）。
- verify 沿用專案既有的 `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`；
  rpm 相關檢查寫成 cargo 整合測試（tests/rpm.rs），缺少 rpmbuild/rpmlint/容器時自動略過，CI 一定要跑。
- 版號只寫在 Cargo.toml，spec 由它讀取並用測試驗證一致；進版需同步 CHANGELOG、badge、tag `vX.Y.Z`。
- 套件相依：scrcpy 不在 Fedora 官方庫（需 COPR zeno/scrcpy），RHEL 系也沒有；使用者決定用 **Requires**。
- rpm 與 tarball 輸出到 `dist/`，不進 git。

## 給下一個 session

1. 沒有未決問題（questions 已清空）。先做 F11，再依「平行執行」表開兩個 worktree subagent 同時做 rpm 與 deb 軌道；F9 查到的倉庫 scrcpy 版本要回寫到 F10 的 README，尤其是「repo 內容如何帶進這個分支」，否則 worktree 無法跑 verify。
2. 動手前載入 `coding-standards`；一個 commit 只做一件事；commit 格式 `<type>: <description>`。
3. 本機沒有 rpmlint，F2 需要時先請使用者安裝（`sudo dnf install rpmlint desktop-file-utils`）或在 CI 跑。
4. 實際發佈與實機驗收（F4 發佈、F6）需要使用者操作，不可假裝測過。

## 進度紀錄

- **F11 完成**（2026-10-10）：`packaging/common/version.sh`（只讀 Cargo.toml 的 `[package]` version）、
  `packaging/common/dex2pc.desktop`（Exec=`dex2pc start`，欄位與 `install-desktop` 產生的一致，只有 Exec 不含絕對路徑）、
  `.gitignore` 加 `dist/`，測試在 `tests/packaging_common.rs`。完整 verify 通過（fmt、clippy、全部測試）。
  下一步：rpm 軌道（F1）與 deb 軌道（F8）可平行開始。
  `desktop-file-validate` 缺少時測試略過，但設了環境變數 `CI` 就會失敗，CI 要安裝 desktop-file-utils。

- **F1 完成**（2026-10-10）：`packaging/rpm/dex2pc.spec`、`packaging/build-rpm.sh`、`tests/rpm.rs`。
  - spec 的版號來自 `%{dex2pc_version}`：build-rpm.sh 用 `--define` 傳入 `packaging/common/version.sh` 的結果；
    直接對 spec 跑 `rpmspec -P`（要在專案根目錄）則自行呼叫同一支腳本，沒有寫死版號。
  - build-rpm.sh 用 `git ls-files` 打包原始碼（含尚未 commit 的修改）、暫存 `_topdir`，成品複製到 `dist/`；
    cargo 若不是 rpm 套件（rustup）會自動加 `--nodeps` 並印出提示。
  - 目前 rpm 只裝 `/usr/bin/dex2pc`；`.desktop`、授權檔、README、`Requires` 都留給 F2。repo 目前沒有 LICENSE 檔，F2 需要處理。
  - 測試：`tests/rpm.rs` 的 build 測試會真的跑 rpmbuild（約 18 秒，要能下載 crate）；缺 rpmbuild 時本機略過，設了 `CI` 就失敗。
  - 下一步：F2（rpm 軌道）或 F8（deb 軌道，可平行）。
- **F1 審查修正**：spec 的 `cargo build` 加 `--target-dir target`，不受 `CARGO_TARGET_DIR` 或 cargo 設定的 `target-dir` 影響；
  `build-rpm.sh` 新增環境變數 `DEX2PC_DIST_DIR`（預設 `dist`，測試用）並只印出這次建出的 rpm。
  回歸測試 `build_script_succeeds_when_cargo_target_dir_points_elsewhere` 在舊 spec 下失敗、修正後通過。

- **F2 完成**（2026-10-10）：rpm 現在裝 `/usr/bin/dex2pc`、`/usr/share/applications/dex2pc.desktop`（取自 `packaging/common/`）、
  `%license LICENSE`、`%doc README.md`；`Requires: android-tools`、`Requires: scrcpy >= 3.2`（下限對應 `src/doctor.rs` 的 `MIN_SCRCPY`，要一起改）。
  - **新增 `LICENSE`**：repo 原本沒有授權檔，`Cargo.toml` 宣告 Apache-2.0，這裡放標準的 Apache-2.0 全文（取自本機 abseil-cpp 套件的同一份標準文字）。
  - spec 加 `BuildRequires: desktop-file-utils` 與 `%check`（desktop-file-validate）；binary 用 `install -s` 去除符號。
  - `packaging/rpm/dex2pc.rpmlintrc`：過濾的項目都寫了原因（scrcpy 拼字、未簽章、Packager/Group、SPDX 授權識別碼；URL 與 man page 附 TODO 條件）。
    rpmlint 結果 0 errors、0 warnings。
  - 測試 `tests/rpm_contents.rs`（6 個）共用一次 build-rpm.sh，用 rpm/rpm2cpio 檢查檔案、相依、packaged 的 .desktop 與 rpmlint；缺工具時本機略過，設了 `CI` 就失敗。
  - **rpmlint 本機沒裝**，我裝在 /tmp/rl 的 venv（`pip install rpmlint`，不在 repo 內）驗證；CI 需要安裝 rpmlint。
  - 下一步：F3（Fedora 容器煙霧測試，需 COPR）或 deb 軌道 F8。
- **F2 審查修正**：新增 `tests/common/mod.rs` 的 `require_tools`（只查 PATH、不執行工具）；build 相關測試先確認 rpmbuild、
  desktop-file-validate（spec 的 `%check`）、git、tar、cargo、rpm、rpm2cpio、cpio 都在，本機缺工具就略過、設了 `CI` 就失敗。
  解包函式 `extract_rpm` 同時檢查 rpm2cpio 與 cpio 的退出狀態（壞掉的 rpm 會讓 cpio 讀到空輸入而成功），有回歸測試。
  共用的 build 產物改放在每個測試程式專屬的 tempdir，由 static fixture 持有；不再使用固定的 `target/rpm-contents-dist`，也沒有吞掉的清理錯誤。
  取捨：static 不會 drop，暫存目錄（約 1 MB）留在系統暫存區由 OS 清理。

- **F3 完成**（2026-10-10）：`packaging/smoke-test.sh [rpm]`：在 `registry.fedoraproject.org/fedora:latest`（可用 `DEX2PC_SMOKE_IMAGE` 換）容器內
  安裝 dnf-plugins-core → `dnf copr enable zeno/scrcpy` → `dnf install` 這個 rpm → `dex2pc --version` 對版號 →
  `dex2pc doctor`（容器沒有手機，預期印出「找不到裝置」且不被訊號殺掉）→ `dnf remove` 後確認 `/usr/bin/dex2pc` 與 `.desktop` 都消失。
  - **已在本機實跑通過**（podman、Fedora 44 映像；COPR zeno/scrcpy 目前提供 scrcpy 5.0.1，滿足 `>= 3.2`；`cargo build --locked` 在 build-rpm.sh 於容器外跑，沒有進容器）。
  - 略過規則：沒有 podman/docker、拉不到映像、容器內連不到 COPR（退出碼 77）→ 本機 exit 0 並印原因；設了 `CI` 則 exit 1。
  - 測試放在 `tests/rpm_contents.rs`：真的跑一次煙霧測試（共用已建好的 rpm，約 40 秒，要網路），以及「沒有容器引擎」本機略過／CI 失敗兩個測試。
  - `dnf remove` 會連同為了 scrcpy 裝進來的相依一起移除（約 300 個套件），輸出很長；腳本已加 `-q`。
  - 下一步：deb 軌道 F8（可與 rpm 軌道的 F5 文件平行），之後 F9。
- **F3 審查修正**：
  - 容器內的步驟拆到 `packaging/smoke-test-inner.sh`（外層 `smoke-test.sh` 掛載進容器執行），好讓判斷邏輯能用假指令測試。
  - doctor 驗證收緊：沒有手機時要求退出碼**剛好是 1** 且輸出含「找不到裝置」；0、101（panic）、128 以上（訊號）或缺訊息都失敗。
  - 網路失敗分類：所有 dnf 步驟（含安裝 rpm 時下載 COPR 的 scrcpy）都經 `run_dnf`，輸出符合已知網路／COPR 無法連線特徵才回 77
    （本機略過、CI 失敗），其餘一般錯誤保持 1。特徵清單寫在 inner 腳本開頭，遇到新的訊息再補。
  - 新增 `tests/smoke_logic.rs`（14 個）：假的 dnf／dex2pc／podman，不需要網路或容器，本機與 CI 都跑。
  - `tests/rpm_contents.rs` 模擬本機的略過測試改成 `env_remove("CI")`；真正的煙霧測試保留繼承的 CI。
  - 已用真實 Fedora 44 容器重跑通過。
- **F3 tester 修正**：`smoke-test-inner.sh` 的網路特徵移除 `Failed to resolve`（dnf5 缺相依時印的是
  `Failed to resolve the transaction`，會被誤判成網路問題而在本機被略過）；也移除 403/404（COPR 名稱打錯也是 404），只收伺服器端 5xx。
  新增 404→1、503→77 兩個測試；完整測試（含真實容器煙霧測試）通過。
- **F3 審查修正（網路分類）**：`smoke-test-inner.sh` 改成只看具體原因。先找一般失敗的證據（HTTP 4xx、`Curl error (22)`、
  `Failed to resolve the transaction`、找不到套件、衝突），有就是失敗；沒有時才看具體連線問題（DNS、連不上、逾時、curl 6/7/28/35/52/56、5xx）。
  不再把籠統的 `Failed to download metadata`／`Cannot download`／`Curl error` 當成網路證據；同一份輸出同時有 DNS 與 404 時以失敗為準（保守）。
  新增 4 個回歸測試（共 25 個），其中 3 個在舊邏輯下失敗。真實容器煙霧測試仍通過。
