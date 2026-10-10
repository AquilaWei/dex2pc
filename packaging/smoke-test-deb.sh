#!/bin/sh
# 在乾淨的 Debian / Ubuntu 容器裡安裝建好的 deb 並實際啟動，確認 deb 在真實環境可用：
# apt install ./dex2pc_*.deb → dex2pc --version → dex2pc doctor → apt remove。
# 倉庫的 scrcpy 不到 3.2 時，改用 equivs 建立版本 3.2 的「假」scrcpy 滿足相依（輸出會註明）。
#
# 用法：packaging/smoke-test-deb.sh <映像> [deb 路徑]
#   映像：例如 debian:13、ubuntu:26.04
#   deb 路徑預設用 dist/ 裡版號對得上的 deb，沒有就要先 build-deb.sh
#
# 沒有容器引擎、拉不到映像或連不到套件倉庫時：本機印出原因並 exit 0（略過）；
# 設了 CI 環境變數則視為失敗，CI 不可略過。
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

if [ $# -lt 1 ]; then
    echo "用法：$0 <映像> [deb 路徑]" >&2
    exit 2
fi
image=$1
# smoke-test-deb-inner.sh 回報「連不到網路或套件倉庫」用的退出碼，和一般失敗區分
network_down=77

skip() {
    echo "smoke-test-deb.sh: 略過：$1" >&2
    if [ -n "${CI:-}" ]; then
        echo "smoke-test-deb.sh: CI 不可略過" >&2
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
deb=${2:-dist/dex2pc_${version}_amd64.deb}
if [ ! -f "$deb" ]; then
    echo "smoke-test-deb.sh: 找不到 dex2pc $version 的 deb（$deb），請先執行 packaging/build-deb.sh" >&2
    exit 1
fi
deb_dir=$(cd "$(dirname "$deb")" && pwd)
deb_name=$(basename "$deb")

# 短名稱（debian:13）不一定被設定成可解析到 docker.io，所以補上完整的倉庫位置
case $image in
*/*) pull_image=$image ;;
*) pull_image=docker.io/library/$image ;;
esac
if ! "$engine" pull --quiet "$pull_image" >/dev/null; then
    skip "拉不到映像 $pull_image"
fi

inner=$root/packaging/smoke-test-deb-inner.sh

echo "smoke-test-deb.sh: 映像 $pull_image"
set +e
# :z 讓 SELinux 允許容器讀掛載的檔案；網路用預設設定即可連外
"$engine" run --rm \
    -v "$deb_dir:/pkg:ro,z" \
    -v "$inner:/smoke-test-deb-inner.sh:ro,z" \
    "$pull_image" sh /smoke-test-deb-inner.sh "$version" "/pkg/$deb_name"
status=$?
set -e

if [ "$status" -eq "$network_down" ]; then
    skip "容器內連不到 $image 的套件倉庫"
fi
exit "$status"
