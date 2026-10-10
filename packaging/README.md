# 打包說明

rpm（Fedora）與 deb（Debian / Ubuntu）共用同一套結構，版號只寫在 `Cargo.toml`。

| 路徑 | 內容 |
|------|------|
| `common/version.sh` | 印出 `Cargo.toml` 的版號，兩種格式的建置腳本都從這裡讀 |
| `common/dex2pc.desktop` | 應用程式選單捷徑（`Exec=dex2pc start`），兩種格式共用 |
| `rpm/` | `dex2pc.spec`、`dex2pc.rpmlintrc`；用 `build-rpm.sh` 建置 |
| `deb/` | `copyright`；用 `build-deb.sh` 建置 |
| `smoke-test.sh` | 在 Fedora 容器內安裝 rpm 並啟動驗證 |
| `check-tag.sh`、`release-notes.sh` | 發佈流程用：確認 tag 是 `vX.Y.0` 且和 `Cargo.toml` 一致；從 `CHANGELOG.md` 取出該版本的段落當 Release notes |
| `smoke-test-deb.sh` | 在 Debian / Ubuntu 容器內安裝 deb 並啟動驗證（`<映像>` 參數） |

輸出一律放到專案根目錄的 `dist/`（已被 git 忽略）。

## deb 的對應

支援 **Debian 13（trixie）與 Ubuntu 26.04 以上**，只有 amd64；`build-deb.sh` 在非 x86_64 主機或執行檔不是 x86-64 ELF 時會拒絕打包。

| 需要 | Fedora（rpm） | Debian / Ubuntu（deb） |
|------|---------------|------------------------|
| adb | `android-tools` | `android-tools-adb`（在 Debian 13 / Ubuntu 26.04 由 `adb` 套件 Provides，`apt` 會自動選它） |
| scrcpy 3.2 以上（強制相依） | `scrcpy >= 3.2`（COPR `zeno/scrcpy`） | `scrcpy (>= 3.2)` |
| C 函式庫 | 由 rpm 自動產生 | `libc6`、`libgcc-s1` |
| 授權檔 | `/usr/share/licenses/dex2pc/LICENSE` | `/usr/share/doc/dex2pc/copyright`（指向 `/usr/share/common-licenses/Apache-2.0`） |

- **scrcpy 的最低版本寫在三個地方**，要一起改：`src/doctor.rs` 的 `MIN_SCRCPY`、`rpm/dex2pc.spec` 的 `Requires`、`build-deb.sh` 的 `min_scrcpy`。
  `tests/rpm_contents.rs` 與 `tests/deb.rs` 會檢查它們一致。
- 實測的倉庫 scrcpy 版本（`smoke-test-deb.sh` 會在輸出回報）：**Debian 13 沒有 scrcpy**；**Ubuntu 26.04 是 3.3.4-1（universe）**，夠新。
  Debian 13 上 `apt install` 會因為強制相依而失敗，使用者要先取得 scrcpy 3.2 以上（README 的安裝段落說明，F10）。
- 其他發行版的倉庫 scrcpy 低於 3.2 時同樣會安裝失敗，處理方式相同。
- 版號沒有 Debian revision（`0.2.0`，不是 `0.2.0-1`）：套件由上游自己建置，不另外維護 Debian 補丁。

## deb 的容器驗證

```bash
packaging/build-deb.sh
packaging/smoke-test-deb.sh debian:13
packaging/smoke-test-deb.sh ubuntu:26.04
```

容器內依序做 `apt install ./dex2pc_*.deb` → `dex2pc --version` → `dex2pc doctor` → `apt remove`。

- **假 scrcpy**：倉庫的 scrcpy 不到 3.2（含 Debian 13 沒有）時，腳本用 `equivs` 建一個版本 3.2 的空套件，名稱和描述都標明 FAKE，輸出也會註明。
  它只證明 dex2pc 的相依能被滿足，不代表 scrcpy 能用；所以 Debian 13 上 `dex2pc doctor` 會回報「缺少 scrcpy」，這是預期的。
- **`doctor` 的安裝提示仍是 Fedora 指令**（`sudo dnf copr enable zeno/scrcpy …`），Debian / Ubuntu 使用者看到時要自行換成 `apt`；這是已決定的範圍，不是 bug。
- 沒有 podman / docker、拉不到映像或連不到套件倉庫時，本機印出原因並 exit 0（略過）；設了 `CI` 環境變數則失敗。
- `lintian`（Debian 13 容器內）對這個 deb 沒有 error，只剩一個 warning：`no-manual-page`（沒有 man page）。
  這次用它檢查時抓到 `changelog.Debian.gz` 的檔名不符 native 套件規則，已改為 `changelog.gz`。lintian 目前沒有放進腳本或 CI。

## CI 與發佈（`.github/workflows/`）

| 檔案 | 觸發 | 做什麼 |
|------|------|--------|
| `ci.yml` | 每次 push（任何分支）與 PR，也被 `release.yml` 重用 | actionlint → fmt、clippy、完整 `cargo test` → 建 rpm 與 deb（含 lintian）並上傳 artifact → 用這份 artifact 跑 rpm（Fedora）、deb（Debian 13、Ubuntu 26.04）三個煙霧測試 |
| `release.yml` | 只有 `v*.*.0` 的 tag | 確認 tag 和 `Cargo.toml` 版號一致 → 呼叫 `ci.yml` → 全部綠燈才建立 GitHub Release，附 rpm、deb、各自的 `.sha256`，notes 取自 CHANGELOG |

- **PATCH 版（`X.Y.Z`，Z 不為 0）不打 tag、不發佈**，但每次 push 都會跑 `ci.yml`，所以 PATCH 版的 rpm 與 deb 在該次 run 的 artifact 裡。
- 只用內建的 `GITHUB_TOKEN`，沒有任何另外設定的 secret；全部跑在 GitHub-hosted 的 `ubuntu-latest`。`contents: write` 只給 release 那個 job。
- CI 的 runner 是 Ubuntu，rpm 用 apt 的 `rpm`／`rpmlint` 建置；本機是 Fedora。兩邊的 `rpmlint` 版本可能不同。
- 發佈步驟：改 `Cargo.toml` 版號與 CHANGELOG → commit → `git tag vX.Y.0 && git push origin vX.Y.0`。

## 建置方式的選擇

候選有 `cargo-deb` 與 `dpkg-deb`，選 **`dpkg-deb`**：

- 不需要額外安裝 crate 或工具，Debian / Ubuntu / Fedora（`dpkg` 套件）都有 `dpkg-deb`
- 檔案樹與 `control` 都由腳本明確寫出，和 rpm 的 spec 一樣好讀、好測
- `--root-owner-group` 不需要 fakeroot

代價：沒有 `dpkg-shlibdeps`，C 函式庫相依只能寫不帶版本的 `libc6`、`libgcc-s1`。

## 本機建置

```bash
packaging/build-rpm.sh   # 需要 rpmbuild、cargo、git、tar、desktop-file-utils
packaging/build-deb.sh   # 需要 dpkg-deb、cargo、git、gzip、strip（binutils）
```
