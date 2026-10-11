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

## 實機驗收清單（F6，使用者操作，確認後才可標為完成）

安裝包的實機項目寫在 README「實機驗收清單」的「安裝包」段，Fedora（rpm）、Debian 13（deb）、Ubuntu 26.04（deb）各做一次：

1. 安裝：照 README 指令裝好，`dex2pc --version` 印出版號
2. 選單出現「Samsung DeX」（不跑 `install-desktop`）
3. 點選單的捷徑開出 DeX 視窗，鍵鼠可操作
4. 移除後選單捷徑消失、`dex2pc` 指令找不到

只能在 GitHub 上確認的項目（沒有 git remote，從沒跑過）：

5. 推一個分支：`CI` workflow 全綠（workflow-lint、verify、package、三個 smoke）
6. 推 PATCH 版 commit：只有 CI artifact，沒有 Release
7. 推 `vX.Y.0` tag：Release 建立，附 rpm、deb 與各自的 `.sha256`，notes 等於 CHANGELOG 該版段落
8. README 的 `../../releases` 連結在 GitHub 上點得到 Release 頁

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
- **F3 審查修正（curl 22）**：`Curl error (22)` 不再單獨當成一般失敗的證據（它只代表 HTTP 回應是錯誤，4xx 與 5xx 都會有）；
  改由實際的 HTTP 狀態決定：4xx 仍優先判為失敗，5xx 仍判為連線問題（77）。新增「完整下載錯誤＋curl 22＋503 → 77」與
  「curl 22 沒有 HTTP 狀態 → 1」兩個測試（共 28 個），前者在舊邏輯下失敗。

- **F8 完成**（2026-10-10）：`packaging/build-deb.sh`（dpkg-deb、`--root-owner-group`、xz）、`packaging/deb/copyright`（DEP-5，指向 common-licenses 的 Apache-2.0）、
  `packaging/README.md`（deb 的對應表與建置工具選擇）、`tests/deb.rs`（11 個）。
  - 先修了 `version.sh` 對 CRLF `Cargo.toml` 的問題（獨立 commit），因為 deb 腳本也靠它取版號。
  - deb：`dex2pc_<版號>_amd64.deb`，Version 就是 Cargo.toml 的版號（沒有 Debian revision）；`Depends: libc6, libgcc-s1, android-tools-adb, scrcpy (>= 3.2)`；
    裝 `/usr/bin/dex2pc`、`/usr/share/applications/dex2pc.desktop`（共用檔）、README、copyright、changelog.Debian.gz；檔案擁有者 root/root。
  - scrcpy 下限現在在三處（`src/doctor.rs`、rpm spec、`build-deb.sh` 的 `min_scrcpy`），`tests/deb.rs` 檢查 doctor 與 build-deb.sh 一致，rpm 的由 `tests/rpm_contents.rs` 檢查。
  - **尚未驗證**：deb 沒有在真實 Debian 13／Ubuntu 26.04 上安裝過（F9 做）；lintian 本機沒裝，沒跑；`libc6` 沒有版本下限（沒有 dpkg-shlibdeps）。
  - 本機實跑：`dpkg-deb -I/-c` 欄位與內容正確；把 scrcpy 改成 Recommends 的變異測試會讓 2 個測試失敗。
  - 下一步：F9（Debian／Ubuntu 容器煙霧測試）；rpm 軌道剩 F5（文件）。

- 修正（審查）：`build-deb.sh` 在非 x86_64 主機直接拒絕，並在打包前用 `readelf -h` 確認執行檔是 x86-64；`tests/deb.rs` 新增 3 個測試（套件內執行檔的 ELF 架構、非 x86_64 主機拒絕、非 x86-64 執行檔拒絕）。

- F9 完成（deb 容器煙霧測試）：
  - 新增 `packaging/smoke-test-deb.sh <映像> [deb]`（外層：引擎、拉映像、掛載；短名稱補成 `docker.io/library/…`）與 `smoke-test-deb-inner.sh`（容器內：`apt-get update` → 回報倉庫 scrcpy 版本 → 必要時 equivs 假 scrcpy 3.2 → `apt install` → `--version` → `doctor` → `apt remove` 並確認檔案消失）。退出碼 77 = 連不到網路／倉庫（本機略過、CI 失敗）。
  - apt 錯誤分類沿用 F3 的兩階段（先找一般失敗證據：404、Unable to locate、unmet dependencies…；沒有才看 DNS／連線／5xx）；內層固定 `LC_ALL=C`；`apt-get update` 加 `APT::Update::Error-Mode=any`，否則部分倉庫抓不到時仍回傳 0。
  - 實跑結果（本機 podman）：Debian 13 與 Ubuntu 26.04 都通過。**倉庫 scrcpy：Debian 13 沒有；Ubuntu 26.04 是 3.3.4-1**。Debian 13 走假 scrcpy（輸出有註明），所以那裡 `doctor` 回報缺 scrcpy 是預期。
  - 發現：Debian 13／Ubuntu 26.04 沒有名為 `android-tools-adb` 的真套件，是 `adb` Provides 它；`Depends: android-tools-adb` 能被 apt 滿足（實測安裝成功），README 已註明。
  - lintian（Debian 13 容器）：0 error；抓到 native 套件的 changelog 應叫 `changelog.gz`，已從 `changelog.Debian.gz` 改名（build-deb.sh、tests/deb.rs）；剩 `no-manual-page` warning。
  - 測試：`tests/smoke_deb_logic.rs`（31 個，假 apt-get／apt-cache／dpkg／equivs-build／dex2pc／podman），變異檢查：把 scrcpy 版本比較 `ge` 改 `gt` 會讓「剛好等於下限」測試失敗；`tests/deb.rs` +1（README 內容）。
  - 下一步：F4（GitHub Actions）；F5、F10 文件；F6 實機。

- F4 完成（GitHub Actions）：
  - `.github/workflows/ci.yml`（push／PR／workflow_call）：`workflow-lint`（docker 跑 actionlint 1.7.7）、`verify`（fmt、clippy、`cargo test --locked`）、`package`（build-rpm、build-deb、lintian、上傳 artifact）、`smoke`（matrix：rpm／Debian 13／Ubuntu 26.04，下載 artifact 後跑煙霧測試）。不含任何 tags／paths 過濾，PATCH 版照跑。
  - `.github/workflows/release.yml`（只有 `v*.*.0` tag）：`check-tag` → `ci`（重用 ci.yml）→ `release`（SHA256、CHANGELOG notes、`gh release create --verify-tag`）；只有 release job 有 `contents: write`，只用 `GITHUB_TOKEN`。
  - `.github/actions/setup-packaging`：安裝 rpm、rpmlint、desktop-file-utils、cpio、binutils、lintian、rustfmt／clippy。
  - `packaging/check-tag.sh`（tag 必須是 `vX.Y.0` 且等於 Cargo.toml 版號）、`packaging/release-notes.sh`（擷取 CHANGELOG 該版本段落，找不到或是空的就失敗）。
  - 測試：`tests/release.rs`（25 個：腳本行為 + workflow 文字結構 + 有 actionlint 時執行它）。本機用 podman 跑 `rhysd/actionlint:1.7.7`，兩個 workflow 與 composite action 都沒有問題。
  - **需要使用者操作／只能在 GitHub 驗證**：(1) 推一般分支確認 `ci.yml` 全綠（尤其 Ubuntu runner 上的 rpm 建置與 rpmlint、runner 上的 podman 能拉 Fedora／Debian／Ubuntu 映像）；(2) 之後 MINOR 進版時推 `vX.Y.0` tag，確認 Release 建立、附件與 `.sha256`、notes；(3) 確認 repo 的 Actions 權限允許 workflow 建立 Release（Settings → Actions → General → Workflow permissions 不能是完全唯讀以外的限制導致 `contents: write` 被拒）。
  - 下一步：F5（rpm 文件）、F10（deb 文件）、F6（實機驗收）。
  - 在 `ubuntu:24.04` 容器（rpm 4.18.2、rpmlint 2.5.0，和 GitHub 的 ubuntu-latest 相同工具鏈）模擬 CI 跑完整測試，抓到兩個 Ubuntu 才有的問題並修好：rpm 的文件／授權目錄會帶版號（`/usr/share/doc/dex2pc-0.2.0`），spec 加 `%global _docdir_fmt %{name}` 與 `_build_id_links none`；無 `%dist` 的檔名 `dex2pc-0.2.0-1.x86_64.rpm` 原本對不上 smoke-test.sh 的 glob，已放寬。修好後該容器內 fmt、clippy、全部測試、build-rpm、build-deb、`lintian --fail-on error` 全過。模擬容器裡沒有 podman，所以容器內安裝的測試在那裡是略過的（GitHub runner 有 podman，且 CI 不可略過）。

- F5 完成（rpm 文件）：
  - README「安裝」拆成「方式一：rpm 安裝包（Fedora x86_64）」與「方式二：cargo install」。rpm 段有可複製的 `sudo dnf copr enable zeno/scrcpy` → `sudo dnf install ./dex2pc-*.rpm`、沒啟用 COPR 會失敗的說明、`.sha256` 檢查、與 cargo install 的差別（捷徑由套件提供）、PATCH 版只有 CI artifact、自建 rpm 步驟（`rpm-build cargo rust desktop-file-utils git tar` + `build-rpm.sh`）、`sudo dnf remove dex2pc`。`install-desktop` 的說明標明只有 cargo install 需要。
  - CHANGELOG [Unreleased] 新增「### 新增」：rpm 安裝包與 CI／Release 規則（使用者看得懂的說法）。
  - 版本 badge 檢查（`examples/sync_version_badge.rs`）照常通過；版號沒有變動（進版是 MINOR 驗收後的事，不在這個功能）。
  - 測試：`tests/docs.rs` +6（指令順序、Fedora only／失敗說明、捷徑差異、自建與移除、PATCH 版、CHANGELOG）。
  - README 的 GitHub Releases 連結用相對路徑 `../../releases`（沒有 git remote 可確認實際網址）。
  - 下一步：F10（deb 文件）、F6（實機驗收）。
  - F5 審查修正：「快速開始」與 doctor／參數範例改成安裝後可直接用的 `dex2pc …`（rpm 使用者不需要 Rust），`cargo run` 只留在「建置與執行」（標明開發用）；建置指令的測試改成寫死 `rpm-build cargo rust desktop-file-utils git tar`，不再從 spec 推導。順手確認 Fedora 的 `rust` 套件 Requires `/usr/bin/cc`，所以不用另外列 gcc。

- F10 完成（deb 文件）：
  - README「安裝」新增「方式二：deb 安裝包」（cargo install 改為方式三）：只支援 Debian 13 / Ubuntu 26.04 以上 amd64；Ubuntu 26.04 直接 `sudo apt install ./dex2pc_*.deb`；Debian 13 先裝 scrcpy 官方靜態版（`/opt/scrcpy` + `/usr/local/bin/scrcpy` 連結），再用 equivs 建「佔位」scrcpy 套件滿足 `Depends`，最後裝 dex2pc；`doctor` 提示只有 Fedora 指令；`.sha256`／PATCH 版說明；`sudo apt remove dex2pc`；自建 deb（`sudo apt install cargo git gzip binutils dpkg` + `build-deb.sh`）。「需求」段落與 platform badge 改為 Fedora / Debian / Ubuntu。
  - 查證（2026-10-11）：Debian trixie、trixie-backports、forky、sid 都**沒有** scrcpy 套件；scrcpy 官方 doc/linux.md 也把 Debian/Ubuntu 的 apt 版標為過時，建議用官方靜態版 `scrcpy-linux-x86_64-vX.tar.gz`（目前 v5.0.1）。
  - 實跑：README 的 Debian 13 步驟在 debian:13 容器完整跑過（官方 scrcpy 5.0.1 + 佔位套件 + dex2pc deb），`dex2pc doctor` 列出 `scrcpy：5.0.1`，移除後 `/usr/bin/dex2pc` 消失。自建 deb 指令在 debian:13（cargo 1.85.1）與 ubuntu:26.04（cargo 1.93.1）容器都建置成功。
  - CHANGELOG [Unreleased]「新增」加一條 deb 安裝包。版本 badge 檢查照常通過。
  - 測試：`tests/docs.rs` +6。
  - 下一步：F6（實機驗收，需要使用者操作）。
  - F10 審查修正：Debian 13 的 scrcpy 步驟改成「每個版本解壓到 `/opt/scrcpy/scrcpy-linux-x86_64-v<版號>/`，`ln -sf` 把 `/usr/local/bin/scrcpy` 指向要用的版本，`scrcpy --version` 確認後才建佔位套件」，避免升級時 `mv` 把新版塞進舊目錄。補上「更新 scrcpy」說明。測試改成寫死 5.0.1，分別檢查下載 URL、解壓目錄與連結、佔位 Version、佔位 deb 檔名、升級說明。
  - 容器驗證（debian:13，照 README 指令）：先裝 3.3.4 + 佔位 3.3.4 + dex2pc（doctor 顯示 3.3.4）→ 不移除 dex2pc 直接升級到 5.0.1（`scrcpy --version`、`dpkg-query` 的佔位版本、doctor 都是 5.0.1）→ 同版本再做一次也正常；dex2pc 一直維持已安裝。

- F6（實機驗收清單）：README「實機驗收清單」新增「安裝包」段（rpm／deb 安裝、選單出現「Samsung DeX」、點擊開出 DeX 視窗、移除後捷徑消失，三個發行版各做一次）；progress.md 新增「實機驗收清單」段，另列 GitHub 上才能確認的 CI／Release 項目。**`passes` 維持 false**：功能步驟規定要使用者實機確認後才能標完成，這裡沒有實機可測。
