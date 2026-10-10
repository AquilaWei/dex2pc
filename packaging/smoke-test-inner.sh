#!/bin/sh
# smoke-test.sh 在 Fedora 容器裡執行的部分：啟用 COPR、安裝 rpm、啟動檢查、移除。
# 用法：smoke-test-inner.sh <預期版號> <rpm 路徑>
# 環境變數 SMOKE_ROOT 只給測試用，改變「移除後檢查檔案是否消失」的根目錄（預設 /）。
#
# 退出碼：0 通過；1 失敗；77 連不到網路或 COPR（呼叫端視為略過，CI 則失敗）。
set -eu

version=$1
rpm=$2
root=${SMOKE_ROOT:-}
network_down=77

# 錯誤輸出的分類只看「具體原因」，不看「Failed to download metadata」「Cannot download」「Curl error」這類
# 籠統字樣——它們在 404、權限、設定錯誤時也會出現，不能證明連線中斷。
#
# 1. 先找一般失敗的證據：有任何一條就是失敗（退出碼 1），即使同一份輸出也有網路字樣。
#    HTTP 4xx（COPR 名稱打錯也是 404）、dnf 解不出相依、找不到套件、衝突。
#    "Failed to resolve the transaction" 是 dnf5 缺相依時的訊息，和網路無關。
other_failure_pattern='Status code: 4[0-9][0-9]|returned error: 4[0-9][0-9]|Curl error \(22\)|Failed to resolve the transaction|Unable to find a match|nothing provides|conflicting requests'
# 2. 沒有一般失敗的證據時，才看有沒有具體的連線問題：DNS 解不出來、連不上、逾時、
#    伺服器端 5xx。curl 的 6=解析主機失敗 7=連不上 28=逾時 35=SSL 連線失敗 52=空回應 56=接收失敗。
network_pattern='Could not resolve host|Temporary failure in name resolution|Network is unreachable|Connection timed out|Connection refused|Failed to connect|Curl error \((6|7|28|35|52|56)\)|Status code: 5[0-9][0-9] for'

# 執行 dnf；成功就不印輸出（-q 之後仍然很長），失敗時把輸出印到 stderr，
# 並依訊息判斷是網路問題（77）還是一般錯誤（1）
run_dnf() {
    log=$(mktemp)
    if "$@" >"$log" 2>&1; then
        rm -f "$log"
        return 0
    fi
    cat "$log" >&2
    if ! grep -qE "$other_failure_pattern" "$log" && grep -qE "$network_pattern" "$log"; then
        rm -f "$log"
        echo "smoke-test-inner.sh: 網路或 COPR 無法連線：$*" >&2
        exit "$network_down"
    fi
    rm -f "$log"
    echo "smoke-test-inner.sh: 指令失敗：$*" >&2
    exit 1
}

run_dnf dnf install -y -q dnf-plugins-core
run_dnf dnf copr enable -y -q zeno/scrcpy

echo "== dnf install"
run_dnf dnf install -y -q "$rpm"

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

echo "== dnf remove"
run_dnf dnf remove -y -q dex2pc
for path in /usr/bin/dex2pc /usr/share/applications/dex2pc.desktop; do
    [ ! -e "$root$path" ] || { echo "移除後 $path 還在" >&2; exit 1; }
done
echo "== smoke test 通過"
