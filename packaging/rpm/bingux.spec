Name:           bingux
Version:        0.1.0
Release:        1%{?dist}
Summary:        Wayland desktop shell for Gnoblin and other layer-shell compositors
License:        GPL-3.0-or-later
URL:            https://github.com/kierandrewett/bingux
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  pkgconfig(libpulse)
BuildRequires:  qt6-qtbase-devel
BuildRequires:  qt6-qtdeclarative-devel
BuildRequires:  qt6-qtbase-private-devel
BuildRequires:  qt6-qttools
BuildRequires:  rust

Requires:       gnome-control-center
Requires:       gnome-session
Requires:       gnome-settings-daemon
Requires:       python3
Requires:       python3-cairo
Requires:       python3-gobject
Requires:       python3-pillow
Requires:       python3-pyyaml
Requires:       quickshell
Requires:       systemd
Requires:       wl-clipboard
Requires:       xdg-utils

%description
Bingux is a Wayland desktop shell. It provides a top bar, dock, search,
notifications, media controls and an optional sidebar. It is a separate
process from the Gnoblin compositor and can also run on other compositors
that provide the layer-shell protocols.

%prep
%autosetup

%build
%make_build BUILD_DIR=build JOBS=%{?_smp_build_ncpus}%{!?_smp_build_ncpus:2}

%install
%make_install PREFIX=%{_prefix} DESTDIR=%{buildroot} BUILD_DIR=build QMLDIR=%{_libdir}/bingux/qml

for unit in bingux.target bingux.service bingux-searchd.service bingux-statusd.service bingux-search-ui.service bingux-switcher-ui.service bingux-capture-ui.service bingux-emoji-ui.service; do
    sed -e 's|/usr/lib64|%{_libdir}|g' packaging/systemd/$unit > %{buildroot}%{_userunitdir}/$unit
done

%files
%license COPYING
%doc README.md docs/extensions.md shell/bingux/README.md
%{_bindir}/bingux
%{_bindir}/bingux-settings
%{_bindir}/binguxctl
%{_bindir}/bingux-audio-meter
%{_bindir}/bingux-searchd
%{_bindir}/bingux-statusd
%{_libdir}/bingux/
%{_libexecdir}/bingux/
%{_datadir}/bingux/
%{_userunitdir}/bingux*.service
%{_userunitdir}/bingux.target

%changelog
* Thu Sep 10 2026 Kieran Drewett <kieran@drewett.dev> - 0.1.0-1
- Add the first native shell package layout.
