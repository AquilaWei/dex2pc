#!/bin/sh
# smoke-test-deb.sh 在 Debian / Ubuntu 容器裡執行的部分：apt update → 回報倉庫的 scrcpy 版本 →
# 必要時以 equivs 建立假的 scrcpy 3.2 → apt install ./dex2pc.deb → dex2pc --version → dex2pc doctor → apt remove。
# 用法：smoke-test-deb-inner.sh <預期版號> <deb 路徑>
# 環境變數 SMOKE_ROOT 只給測試用，改變「移除後檢查檔案是否消失」的根目錄（預設 /）。
#
# 退出碼：0 通過；1 失敗；77 連不到網路或套件倉庫（呼叫端視為略過，CI 則失敗）。
set -eu

version=$1
deb=$2
root=${SMOKE_ROOT:-}
network_down=77
# 和 build-deb.sh 的 min_scrcpy、src/doctor.rs 的 MIN_SCRCPY 同一個值
min_scrcpy=3.2

# apt 的訊息會被翻譯，固定成英文才能比對；DEBIAN_FRONTEND 避免安裝時等人回答
export LC_ALL=C DEBIAN_FRONTEND=noninteractive

# 分類方式同 smoke-test-inner.sh：先找一般失敗的證據，有就是失敗（1）；沒有才看具體的連線問題（77）。
# 「Failed to fetch」「Some index files failed to download」404、5xx、DNS 都會出現，不能單獨當網路證據。
other_failure_pattern='404 +Not Found|Unable to locate package|has no installation candidate|unmet dependencies|Unmet dependencies|but it is not installable|but it is not going to be installed|Unable to correct problems|dpkg: error'
network_pattern='Could not resolve|Temporary failure resolving|Temporary failure in name resolution|Network is unreachable|Connection timed out|Connection failed|Could not connect|Cannot initiate the connection|50[0-9] +(Internal Server Error|Bad Gateway|Service Unavailable|Gateway Time-out)'

# 執行指令；成功就不印輸出，失敗時把輸出印到 stderr，並依訊息判斷是網路問題（77）還是一般錯誤（1）
run_apt() {
    log=$(mktemp)
    if "$@" >"$log" 2>&1; then
        rm -f "$log"
        return 0
    fi
    cat "$log" >&2
    if ! grep -qE "$other_failure_pattern" "$log" && grep -qE "$network_pattern" "$log"; then
        rm -f "$log"
        echo "smoke-test-deb-inner.sh: 網路或套件倉庫無法連線：$*" >&2
        exit "$network_down"
    fi
    rm -f "$log"
    echo "smoke-test-deb-inner.sh: 指令失敗：$*" >&2
    exit 1
}

# 預設 apt-get update 只有部分倉庫抓不到時仍回傳 0，會讓後面的錯誤難以判斷
echo "== apt-get update"
run_apt apt-get -o APT::Update::Error-Mode=any update -qq

. "$root/etc/os-release"
# 倉庫的版本，給 README 說明怎麼取得 scrcpy 的依據
# 先完整取得輸出再解析：寫成管線的話 apt-cache 失敗會被吞掉，被當成「倉庫沒有這個套件」
# （沒有這個套件時 apt-cache policy 仍是退出 0，只是不印東西）
policy_version() {
    policy=$(apt-cache policy "$1") || {
        echo "smoke-test-deb-inner.sh: apt-cache policy $1 失敗" >&2
        exit 1
    }
    printf '%s\n' "$policy" | sed -n 's/^ *Candidate: *//p'
}
repo_scrcpy=$(policy_version scrcpy)
# android-tools-adb 在 Debian 13 / Ubuntu 26.04 是 adb 套件 Provides 的虛擬名稱，要看 adb 的版本
repo_adb=$(policy_version adb)
echo "== 映像：$PRETTY_NAME"
echo "== 倉庫的 scrcpy 版本：${repo_scrcpy:-（沒有）}（需要 >= $min_scrcpy）"
echo "== 倉庫的 adb 版本（提供 android-tools-adb）：${repo_adb:-（沒有）}"

if [ "$repo_scrcpy" = "(none)" ]; then
    repo_scrcpy=
fi
if [ -n "$repo_scrcpy" ] && dpkg --compare-versions "$repo_scrcpy" ge "$min_scrcpy"; then
    echo "== 倉庫的 scrcpy 夠新，使用真的 scrcpy"
else
    echo "== 倉庫的 scrcpy 不到 $min_scrcpy：改用 equivs 建立「假的」scrcpy $min_scrcpy 套件來滿足相依"
    echo "== 注意：這不是真的 scrcpy，只能證明 dex2pc 的相依能被滿足，不能證明 scrcpy 能用"
    run_apt apt-get install -y -qq equivs
    fake_dir=$(mktemp -d)
    cat >"$fake_dir/scrcpy-fake" <<CONTROL
Section: misc
Priority: optional
Standards-Version: 4.6.2
Package: scrcpy
Version: $min_scrcpy
Maintainer: smoke test <nobody@example.invalid>
Description: FAKE scrcpy $min_scrcpy for the dex2pc smoke test
 Empty package built with equivs. It is not scrcpy and only satisfies the
 dependency of dex2pc.
CONTROL
    (cd "$fake_dir" && run_apt equivs-build scrcpy-fake)
    run_apt apt-get install -y -qq "$fake_dir/scrcpy_${min_scrcpy}_all.deb"
fi

# 掛載進來的目錄是唯讀且 _apt 使用者讀不到，先複製到容器內
local_deb=$(mktemp -d)/$(basename "$deb")
cp "$deb" "$local_deb"
chmod 0644 "$local_deb"

echo "== apt-get install ./$(basename "$deb")"
run_apt apt-get install -y -qq "$local_deb"

echo "== dex2pc --version"
actual=$(dex2pc --version)
[ "$actual" = "dex2pc $version" ] || { echo "版本不符：$actual" >&2; exit 1; }

echo "== dex2pc doctor（容器裡沒有手機，預期退出碼 1 並回報找不到裝置）"
set +e
doctor=$(dex2pc doctor 2>&1)
status=$?
set -e
echo "$doctor"
# 沒有手機時 doctor 的正常退出碼是 1；0 表示它誤以為有手機，101 是 Rust panic，128 以上是被訊號殺掉
[ "$status" -eq 1 ] || { echo "doctor 退出碼應為 1，實際是 $status" >&2; exit 1; }
echo "$doctor" | grep -q "找不到裝置" || { echo "doctor 沒有回報「找不到裝置」" >&2; exit 1; }

echo "== apt-get remove dex2pc"
run_apt apt-get remove -y -qq dex2pc
for path in /usr/bin/dex2pc /usr/share/applications/dex2pc.desktop; do
    [ ! -e "$root$path" ] || { echo "移除後 $path 還在" >&2; exit 1; }
done
echo "== smoke test 通過"
