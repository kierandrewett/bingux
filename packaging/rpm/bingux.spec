Name:           bingux
Version:        0.1.0
Release:        1%{?dist}
Summary:        Wayland desktop shell for Gnoblin and other layer-shell compositors
License:        GPL-3.0-or-later AND LGPL-3.0-or-later AND CC-BY-SA-3.0
URL:            https://github.com/kierandrewett/bingux
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  pkgconfig(gio-2.0)
BuildRequires:  pkgconfig(libpulse)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  wayland-devel
BuildRequires:  pkgconfig(egl)
BuildRequires:  pkgconfig(fontconfig)
BuildRequires:  pkgconfig(freetype2)
BuildRequires:  pkgconfig(gl)
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  qt6-qtbase-devel
BuildRequires:  qt6-qtdeclarative-devel
BuildRequires:  qt6-qtwayland-devel
BuildRequires:  qt6-qtbase-private-devel
BuildRequires:  qt6-qttools
BuildRequires:  rust
BuildRequires:  systemd-rpm-macros
# bingux-shell: Quickshell (vendored in packages/bingux-shell) built into the
# Bingux binary with CMake and Ninja.
BuildRequires:  cmake
BuildRequires:  ninja-build
BuildRequires:  cli11-devel
BuildRequires:  jemalloc-devel
BuildRequires:  pkgconfig(libarchive)
BuildRequires:  pkgconfig(libpipewire-0.3)
BuildRequires:  pkgconfig(libdrm)
BuildRequires:  pkgconfig(polkit-agent-1)
BuildRequires:  pkgconfig(xcb)
BuildRequires:  pkgconfig(wayland-protocols)
BuildRequires:  pam-devel
BuildRequires:  qt6-qtshadertools-devel
BuildRequires:  qt6-qtsvg-devel
BuildRequires:  spirv-tools

Requires:       gnome-settings-daemon
Requires:       gtk3
Requires:       gtk4
Requires:       libadwaita
Requires:       fontconfig
Requires:       mesa-libEGL
Requires:       python3
Requires:       python3-cairo
Requires:       python3-gobject
Requires:       python3-pillow
Requires:       python3-pyyaml
Requires:       /usr/bin/ffmpeg
# The Qt QML runtime bingux-shell links against; Quickshell itself is inside
# the binary.
Requires:       qt6-qtdeclarative
Requires:       qt6-qtwayland
Requires:       qt6-qtsvg
Requires:       systemd
Requires:       wl-clipboard
Requires:       xdg-utils
# Desktop-wide settings (accent, colour scheme, background, idle delay) live
# in GSettings; dconf keeps them across logins. glib2 provides gsettings.
Requires:       dconf
Requires:       glib2
# Backends the Settings application drives. Each page reports a readable
# error when its tool is missing, so these are recommendations.
Recommends:     NetworkManager
Recommends:     accountsservice
Recommends:     bluez
Recommends:     power-profiles-daemon
Recommends:     upower
Recommends:     wireplumber
Recommends:     zenity

%systemd_requires

%description
Bingux is a Wayland desktop shell. It provides a top bar, dock, search,
notifications, media controls and an optional sidebar. It is a separate
process from the Gnoblin compositor and can also run on other compositors
that provide the layer-shell protocols.

%prep
%autosetup

%build
%make_build BUILD_DIR=build JOBS=%{?_smp_build_ncpus}%{!?_smp_build_ncpus:2} LIBDIR_NAME=%{_lib}

%install
%make_install PREFIX=%{_prefix} DESTDIR=%{buildroot} BUILD_DIR=build QMLDIR=%{_libdir}/bingux/qml

for unit in bingux.target bingux.service bingux-searchd.service bingux-statusd.service bingux-search-ui.service bingux-switcher-ui.service bingux-capture-ui.service bingux-emoji-ui.service bingux-lock.service bingux-greeter-export.service bingux-greeter-export.path; do
    sed -e 's|/usr/lib64|%{_libdir}|g' packaging/systemd/$unit > %{buildroot}%{_userunitdir}/$unit
done
install -Dpm 0644 packaging/pam/bingux-lock %{buildroot}%{_sysconfdir}/pam.d/bingux-lock

%post
%systemd_user_post bingux.target
%preun
%systemd_user_preun bingux.target bingux-searchd.service bingux-statusd.service

%postun
%systemd_user_postun_with_restart bingux.target bingux-searchd.service bingux-statusd.service

%files
%license COPYING
%license %{_datadir}/licenses/adwaita-icon-theme/COPYING
%license %{_datadir}/licenses/adwaita-icon-theme/COPYING_CCBYSA3
%license %{_datadir}/licenses/adwaita-icon-theme/COPYING_LGPL
%license packages/bingux-shell/quickshell/LICENSE
%doc README.md docs/extensions.md docs/locking.md docs/login-screen.md
%{_bindir}/bingux-shell
%{_bindir}/bingux
%{_bindir}/bingux-capture-ui
%{_bindir}/bingux-lock
%{_bindir}/bingux-emoji-ui
%{_bindir}/bingux-greeter
%{_bindir}/bingux-greeter-session
%{_bindir}/bingux-session
%{_bindir}/bingux-wallpaper
%{_bindir}/bingux-popouts
%{_bindir}/bingux-search-ui
%{_bindir}/bingux-switcher-ui
%{_bindir}/binguxctl
%{_bindir}/bingux-audio-meter
%{_bindir}/bingux-frame
%{_bindir}/bingux-clipboard-paste
%{_bindir}/bingux-searchd
%{_bindir}/bingux-search-service
%{_bindir}/bingux-statusd
%{_bindir}/bingux-config
%{_bindir}/bingux-settings
%{_datadir}/applications/bingux-settings.desktop
%{_libdir}/bingux/
%{_libexecdir}/bingux/
%{_datadir}/bingux/
%{_datadir}/gnoblin/conf.d/bingux.lua
%{_datadir}/gnoblin/scripts/bingux-text-input.js
%{_userunitdir}/bingux*.service
%{_userunitdir}/bingux*.path
%{_userunitdir}/bingux.target
%{_datadir}/polkit-1/rules.d/50-bingux-greeter.rules
%{_tmpfilesdir}/bingux-greeter.conf
%config(noreplace) %{_sysconfdir}/pam.d/bingux-lock

%changelog
* Thu Sep 10 2026 Kieran Drewett <kieran@drewett.dev> - 0.1.0-1
- Add the first native shell package layout.
