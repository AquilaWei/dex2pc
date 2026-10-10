#!/bin/sh
# 在本機用 dpkg-deb 產出 dex2pc 的 .deb（amd64，Debian 13 / Ubuntu 26.04 以上），放到專案根目錄的 dist/。
# 輸出目錄可用環境變數 DEX2PC_DIST_DIR 改（測試用）。最後一行印出這次建出的 .deb 路徑。
# 版號取自 packaging/common/version.sh；.desktop 與 rpm 共用 packaging/common/。
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(sh packaging/common/version.sh)
# scrcpy 最低版本：和 src/doctor.rs 的 MIN_SCRCPY、rpm spec 的 Requires 同一個值，三處要一起改
min_scrcpy=3.2
maintainer="AquilaWei <weichuangtaiwan@gmail.com>"

# 套件固定標 amd64：非 x86_64 主機直接拒絕，不產出架構標錯的套件
host_arch=$(uname -m)
if [ "$host_arch" != x86_64 ]; then
    echo "build-deb.sh only builds amd64 packages on x86_64 hosts (this host: $host_arch)" >&2
    exit 1
fi

top=$(mktemp -d)
trap 'rm -rf "$top"' EXIT
pkg=$top/pkg

# --target-dir 明確指定：不受使用者的 CARGO_TARGET_DIR 或 cargo 設定影響
cargo build --release --locked --target-dir "$root/target"

# 打包前再確認執行檔真的是 x86-64 ELF（例如 .cargo/config 指定了別的 target 時會不符）
machine=$(LC_ALL=C readelf -h target/release/dex2pc | sed -n 's/^ *Machine: *//p')
if [ "$machine" != "Advanced Micro Devices X86-64" ]; then
    echo "target/release/dex2pc is not an x86-64 binary (Machine: $machine)" >&2
    exit 1
fi

install -D -s -m 0755 target/release/dex2pc "$pkg/usr/bin/dex2pc"
install -D -m 0644 packaging/common/dex2pc.desktop "$pkg/usr/share/applications/dex2pc.desktop"
install -D -m 0644 README.md "$pkg/usr/share/doc/dex2pc/README.md"
install -D -m 0644 packaging/deb/copyright "$pkg/usr/share/doc/dex2pc/copyright"

# Debian 要求的 changelog；日期取最後一次 commit 的時間，同樣的 commit 建出同樣的內容
{
    printf 'dex2pc (%s) unstable; urgency=medium\n\n' "$version"
    printf '  * Release %s. See CHANGELOG.md in the source tree for details.\n\n' "$version"
    printf ' -- %s  %s\n' "$maintainer" "$(git log -1 --format=%cD)"
} | gzip -9n >"$pkg/usr/share/doc/dex2pc/changelog.Debian.gz"
chmod 0644 "$pkg/usr/share/doc/dex2pc/changelog.Debian.gz"

installed_size=$(du -sk "$pkg" | cut -f1)
mkdir -p "$pkg/DEBIAN"
# Depends：adb 與 scrcpy 是強制相依。scrcpy 不夠新時 apt 會拒絕安裝，README 說明如何取得新版
cat >"$pkg/DEBIAN/control" <<CONTROL
Package: dex2pc
Version: $version
Architecture: amd64
Maintainer: $maintainer
Installed-Size: $installed_size
Depends: libc6, libgcc-s1, android-tools-adb, scrcpy (>= $min_scrcpy)
Section: utils
Priority: optional
Description: Show Samsung DeX as a window on a Linux PC over USB-C
 dex2pc shows Samsung DeX as a window on a Linux PC over a USB-C cable,
 driven by scrcpy. It checks the phone and the tools first, then starts
 scrcpy with a virtual display so the keyboard and mouse of the PC control
 DeX.
CONTROL

dist=${DEX2PC_DIST_DIR:-dist}
mkdir -p "$dist"
deb=$dist/dex2pc_${version}_amd64.deb
# --root-owner-group：不需要 fakeroot，檔案擁有者直接記成 root；xz 壓縮讓舊版 dpkg 也能讀
dpkg-deb --root-owner-group -Zxz --build "$pkg" "$deb" >&2
echo "$deb"
