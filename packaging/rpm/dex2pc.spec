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

%description
dex2pc shows Samsung DeX as a window on a Linux PC over a USB-C cable,
driven by scrcpy. It checks the phone and the tools first, then starts scrcpy
with a virtual display so the keyboard and mouse of the PC control DeX.

%prep
%autosetup

%build
cargo build --release --locked

%install
install -D -m 0755 target/release/dex2pc %{buildroot}%{_bindir}/dex2pc

%files
%{_bindir}/dex2pc

%changelog
* Sat Oct 10 2026 AquilaWei <weichuangtaiwan@gmail.com> - %{version}-1
- Initial rpm package
