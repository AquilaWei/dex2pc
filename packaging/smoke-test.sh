#!/bin/sh
# 在乾淨的 Fedora 容器裡安裝建好的 rpm 並實際啟動，確認 rpm 在真實環境可用：
# 啟用 COPR zeno/scrcpy（強制相依需要）→ dnf install → dex2pc --version → dex2pc doctor → dnf remove。
#
# 用法：packaging/smoke-test.sh [rpm 路徑]   （預設用 dist/ 裡版號對得上的 rpm，沒有就先 build-rpm.sh）
# 環境變數：DEX2PC_SMOKE_IMAGE 換容器映像（預設 registry.fedoraproject.org/fedora:latest）
#
# 沒有容器引擎、拉不到映像或連不到 COPR 時：本機印出原因並 exit 0（略過）；
# 設了 CI 環境變數則視為失敗，CI 不可略過。
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

image=${DEX2PC_SMOKE_IMAGE:-registry.fedoraproject.org/fedora:latest}
# 容器內回報「網路不通」用的退出碼，和一般失敗區分
network_down=77

skip() {
    echo "smoke-test.sh: 略過：$1" >&2
    if [ -n "${CI:-}" ]; then
        echo "smoke-test.sh: CI 不可略過" >&2
        exit 1
    fi
    exit 0
}

if command -v podman >/dev/null 2>&1; then
    engine=podman
elif command -v docker >/dev/null 2>&1; then
    engine=docker
else
    skip "找不到 podman 或 docker"
fi

version=$(sh packaging/common/version.sh)
rpm=${1:-}
if [ -z "$rpm" ]; then
    # 檔名是 dex2pc-<版號>-1.<dist>.x86_64.rpm；只取版號對得上的，避免誤用舊版本的產物
    for candidate in "dist/dex2pc-$version-1."*.x86_64.rpm; do
        [ -f "$candidate" ] && rpm=$candidate
    done
fi
if [ -z "$rpm" ] || [ ! -f "$rpm" ]; then
    echo "smoke-test.sh: 找不到 dex2pc $version 的 rpm，請先執行 packaging/build-rpm.sh" >&2
    exit 1
fi
rpm_dir=$(cd "$(dirname "$rpm")" && pwd)
rpm_name=$(basename "$rpm")

if ! "$engine" pull --quiet "$image" >/dev/null; then
    skip "拉不到映像 $image"
fi

set +e
# :z 讓 SELinux 允許容器讀掛載目錄；--network 預設即可連外
"$engine" run --rm -v "$rpm_dir:/pkg:ro,z" "$image" sh -eu -c '
    version=$1
    rpm=/pkg/$2
    network_down=$3

    # copr 外掛與 COPR 都要連網；這兩步失敗視為網路問題，不是套件問題
    dnf install -y -q dnf-plugins-core || exit "$network_down"
    dnf copr enable -y -q zeno/scrcpy || exit "$network_down"

    echo "== dnf install"
    dnf install -y -q "$rpm"

    echo "== dex2pc --version"
    actual=$(dex2pc --version)
    [ "$actual" = "dex2pc $version" ] || { echo "版本不符：$actual" >&2; exit 1; }

    echo "== dex2pc doctor（容器裡沒有手機，預期回報沒有裝置、但不能 crash）"
    set +e
    doctor=$(dex2pc doctor 2>&1)
    status=$?
    set -e
    echo "$doctor"
    # 1 是 doctor 檢查未通過的正常退出；128 以上是被訊號殺掉（crash）
    [ "$status" -lt 128 ] || { echo "doctor 異常結束，退出碼 $status" >&2; exit 1; }
    echo "$doctor" | grep -q "裝置" || { echo "doctor 沒有提到手機／裝置" >&2; exit 1; }

    echo "== dnf remove"
    dnf remove -y -q dex2pc
    for path in /usr/bin/dex2pc /usr/share/applications/dex2pc.desktop; do
        [ ! -e "$path" ] || { echo "移除後 $path 還在" >&2; exit 1; }
    done
    echo "== smoke test 通過"
' sh "$version" "$rpm_name" "$network_down"
status=$?
set -e

if [ "$status" -eq "$network_down" ]; then
    skip "容器內連不到 COPR zeno/scrcpy"
fi
exit "$status"
