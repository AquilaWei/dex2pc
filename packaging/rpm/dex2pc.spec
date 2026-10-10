# Version 由 packaging/common/version.sh 從 Cargo.toml 讀；build-rpm.sh 會用 --define 傳入。
# 直接對這個檔案跑 rpmspec 時（要在專案根目錄）則用下面這行取同一個值。
%{!?dex2pc_version:%global dex2pc_version %(sh packaging/common/version.sh)}

# Rust 的 binary 沒有 rpm 需要的 build-id 之外的除錯資訊，不另外產生 debuginfo 套件
%global debug_package %{nil}

Name:           dex2pc
Version:        %{dex2pc_version}
Release:        1%{?dist}
Summary:        Show Samsung DeX as a window on a Linux PC over USB-C
License:        Apache-2.0
ExclusiveArch:  x86_64
Source0:        %{name}-%{version}.tar.gz

# edition 2024 需要 Rust 1.85 以上
BuildRequires:  cargo >= 1.85
BuildRequires:  rust >= 1.85
BuildRequires:  desktop-file-utils

# dex2pc 啟動時呼叫 adb 與 scrcpy。scrcpy 不在 Fedora 官方庫，要先啟用 COPR zeno/scrcpy；
# 3.2 是程式內檢查的最低版本（src/doctor.rs 的 MIN_SCRCPY），兩邊要一起改
Requires:       android-tools
Requires:       scrcpy >= 3.2

%description
dex2pc shows Samsung DeX as a window on a Linux PC over a USB-C cable,
driven by scrcpy. It checks the phone and the tools first, then starts scrcpy
with a virtual display so the keyboard and mouse of the PC control DeX.

%prep
%autosetup

%build
# 明確指定 --target-dir：不受使用者的 CARGO_TARGET_DIR 或 cargo 設定的 target-dir 影響，
# 讓 %install 讀的路徑和建置輸出一致
cargo build --release --locked --target-dir target

%install
install -D -s -m 0755 target/release/dex2pc %{buildroot}%{_bindir}/dex2pc
install -D -m 0644 packaging/common/dex2pc.desktop %{buildroot}%{_datadir}/applications/dex2pc.desktop

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/dex2pc.desktop

%files
%license LICENSE
%doc README.md
%{_bindir}/dex2pc
%{_datadir}/applications/dex2pc.desktop

%changelog
* Sat Oct 10 2026 AquilaWei <weichuangtaiwan@gmail.com> - %{version}-1%{?dist}
- Initial rpm package
