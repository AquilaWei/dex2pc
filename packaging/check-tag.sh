#!/bin/sh
# 發佈流程用：確認 tag 是 MINOR 以上的版本（vX.Y.0），且和 Cargo.toml 的版號一致。
# 用法：packaging/check-tag.sh <tag>      例：packaging/check-tag.sh v0.3.0
# 不符時印出原因並 exit 1。
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)

if [ $# -ne 1 ]; then
    echo "用法：$0 <tag>" >&2
    exit 2
fi
tag=$1

# 只接受 vX.Y.0：PATCH 進版（Z 不為 0）不打 tag、不發佈
case $tag in
v[0-9]*.[0-9]*.0) ;;
*)
    echo "check-tag.sh: tag $tag 不是 vX.Y.0（PATCH 版不發佈）" >&2
    exit 1
    ;;
esac
# 上面的 glob 擋不掉 v1.2.x.0、v1.-2.0 這類多餘的點或符號，所以再用嚴格的格式確認一次
if ! printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.0$'; then
    echo "check-tag.sh: tag $tag 格式不是 vX.Y.0" >&2
    exit 1
fi

version=$(sh "$root/packaging/common/version.sh")
if [ "v$version" != "$tag" ]; then
    echo "check-tag.sh: tag $tag 與 Cargo.toml 的版號 $version 不一致" >&2
    exit 1
fi
