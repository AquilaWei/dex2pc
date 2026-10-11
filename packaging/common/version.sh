#!/bin/sh
# 印出 Cargo.toml [package] 的 version。版號只寫在 Cargo.toml，rpm 與 deb 的建置腳本都從這裡讀。
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)

# 只讀 [package] 區段，避免抓到依賴表裡的 version = "..."。
# 容忍 CRLF 換行、行首縮排、區段標題後的註解、單引號或雙引號的字串、值後面的註解（\047 是單引號）。
version=$(awk '
    {
        line = $0
        sub(/\r$/, "", line)  # CRLF 換行的 Cargo.toml
        sub(/^[ \t]+/, "", line)
    }
    line ~ /^\[/ {
        header = line
        sub(/[ \t]*(#.*)?$/, "", header)
        in_package = (header == "[package]")
        next
    }
    in_package && line ~ /^version[ \t]*=/ {
        sub(/^version[ \t]*=[ \t]*/, "", line)
        quote = substr(line, 1, 1)
        if (quote != "\"" && quote != "\047") next
        rest = substr(line, 2)
        end = index(rest, quote)
        if (end > 1) {
            print substr(rest, 1, end - 1)
            exit
        }
    }
' "$root/Cargo.toml")

if [ -z "$version" ]; then
    echo "version.sh: 在 $root/Cargo.toml 找不到 [package] version" >&2
    exit 1
fi
printf '%s\n' "$version"
