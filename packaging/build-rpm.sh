#!/bin/sh
# 在本機用 rpmbuild 產出 dex2pc 的 binary rpm，放到專案根目錄的 dist/。
# 輸出目錄預設是 dist/，可用環境變數 DEX2PC_DIST_DIR 改（測試用，避免多個建置互相覆蓋）。
# 原始碼取自 git 追蹤的檔案（含尚未 commit 的修改），建置過程用暫存的 rpmbuild 目錄，不動 ~/rpmbuild。
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(sh packaging/common/version.sh)
name=dex2pc

top=$(mktemp -d)
trap 'rm -rf "$top"' EXIT
mkdir -p "$top/SOURCES" "$top/SPECS"

git ls-files -z | tar --null -T - --transform "s,^,$name-$version/," -czf "$top/SOURCES/$name-$version.tar.gz"
cp packaging/rpm/dex2pc.spec "$top/SPECS/"

# 用 rustup 裝的 cargo 不是 rpm 套件，BuildRequires 會誤判成缺少；有 cargo 就略過相依檢查
nodeps=
if ! rpm -q --quiet cargo && command -v cargo >/dev/null 2>&1; then
    echo "build-rpm.sh: cargo 不是 rpm 套件（可能來自 rustup），略過 BuildRequires 檢查" >&2
    nodeps=--nodeps
fi

# shellcheck disable=SC2086 # nodeps 為空時不能被當成空字串參數
rpmbuild -bb $nodeps \
    --define "_topdir $top" \
    --define "dex2pc_version $version" \
    "$top/SPECS/dex2pc.spec"

dist=${DEX2PC_DIST_DIR:-dist}
mkdir -p "$dist"
# 只印出這次建出來的檔案，dist/ 裡以前的產物不列出
for rpm in "$top"/RPMS/*/*.rpm; do
    cp "$rpm" "$dist/"
    echo "$dist/$(basename "$rpm")"
done
