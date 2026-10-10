#!/bin/sh
# 印出 Cargo.toml [package] 的 version。版號只寫在 Cargo.toml，rpm 與 deb 的建置腳本都從這裡讀。
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)

# 只讀 [package] 區段，避免抓到依賴表裡的 version = "..."
version=$(awk '
    /^\[/ { in_package = ($0 == "[package]"); next }
    in_package && /^version[ \t]*=/ { gsub(/^[^"]*"|".*$/, ""); print; exit }
' "$root/Cargo.toml")

if [ -z "$version" ]; then
    echo "version.sh: 在 $root/Cargo.toml 找不到 [package] version" >&2
    exit 1
fi
printf '%s\n' "$version"
