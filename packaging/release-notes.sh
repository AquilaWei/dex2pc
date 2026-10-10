#!/bin/sh
# 發佈流程用：印出 CHANGELOG.md 裡某個版本的段落（不含 "## [版號]" 那一行），作為 Release notes。
# 用法：packaging/release-notes.sh <版號>      例：packaging/release-notes.sh 0.3.0
# 環境變數 DEX2PC_CHANGELOG 只給測試用，換成別的 CHANGELOG 檔。
# CHANGELOG 沒有該版本、或該版本的段落是空的就 exit 1，避免發出沒有說明的 Release。
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)

if [ $# -ne 1 ]; then
    echo "用法：$0 <版號>" >&2
    exit 2
fi
version=$1
changelog=${DEX2PC_CHANGELOG:-$root/CHANGELOG.md}

notes=$(awk -v header="## [$version]" '
    index($0, header) == 1 { found = 1; next }
    found && /^## \[/ { exit }
    found { print }
' "$changelog")

# 去掉前後的空行後只剩空白，等於沒有內容
if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
    echo "release-notes.sh: CHANGELOG.md 找不到版本 $version 的內容" >&2
    exit 1
fi
printf '%s\n' "$notes" | awk 'NF { started = 1 } started'
