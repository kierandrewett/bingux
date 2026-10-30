# One source package, seven binary packages:
#
#   bingux               the desktop shell, its helper daemons and user units
#   bingux-config        the configuration CLI the shell and Settings share
#   bingux-settings      Settings
#   bingux-files         Files
#   bingux-preview-host  the helper Files runs to play media and render PDFs
#   bingux-audio-effects the sound effects engine and its LADSPA plugins
#   bingux-taxonomy      the Bingux UI reference window (development tool)
#
# `make install` stages everything below the build root; each files list picks
# its part. rpmbuild fails on an installed file that no list claims.

# Rust flags: Fedora's %%{build_rustflags} without codegen-units=1 and with
# line-table debug info, which keeps the debuginfo of the statically linked
# Freya/Skia programs to a size the debuginfo packages can hold.
%global bingux_rustflags -Copt-level=3 -Cdebuginfo=line-tables-only -Cstrip=none -Cforce-frame-pointers=yes --cap-lints=warn

# The licence of a package is the project's GPL plus the licences of the
# crates linked into it: scripts/cargo-licenses.py computes the expressions
# and tests/rpm-package.py checks them.
Name:           bingux
Version:        0.1.0
Release:        1%{?dist}
Summary:        Wayland desktop shell for Gnoblin and other layer-shell compositors
License:        Apache-2.0 AND BSD-3-Clause AND CC-BY-SA-3.0 AND CDLA-Permissive-2.0 AND GPL-3.0-or-later AND ISC AND LGPL-3.0-or-later AND MIT AND Unicode-3.0 AND Zlib
URL:            https://github.com/kierandrewett/bingux
Source0:        %{name}-%{version}.tar.xz

BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  python3
BuildRequires:  python3-devel
BuildRequires:  util-linux
BuildRequires:  desktop-file-utils
BuildRequires:  appstream
BuildRequires:  pkgconfig(gio-2.0)
BuildRequires:  pkgconfig(gstreamer-1.0)
BuildRequires:  pkgconfig(gstreamer-app-1.0)
BuildRequires:  pkgconfig(gstreamer-video-1.0)
BuildRequires:  pkgconfig(poppler-glib)
BuildRequires:  pkgconfig(cairo)
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

Requires:       bingux-config = %{version}-%{release}
Requires:       gtk3
Requires:       gtk4
Requires:       libadwaita-1.so.0()(64bit)
Requires:       fontconfig
Requires:       mesa-libEGL
Requires:       python3
Requires:       python3-cairo
Requires:       python3-gobject
Requires:       python3-pillow
Requires:       /usr/bin/ffmpeg
# The Qt QML runtime bingux-shell links against; Quickshell itself is inside
# the binary.
Requires:       qt6-qtdeclarative
Requires:       qt6-qtwayland
Requires:       qt6-qtsvg
Requires:       systemd
Requires:       wl-clipboard
Requires:       xdg-desktop-portal
Requires:       xdg-utils
# Desktop-wide settings (accent, colour scheme, background, idle delay) live
# in GSettings; dconf keeps them across logins. glib2 provides gsettings.
Requires:       dconf
Requires:       /usr/bin/gsettings
# Night light state in the Control Centre reads gnome-settings-daemon and
# reports "off" without it; YAML frontmatter in document previews needs
# PyYAML and shows "could not be parsed" without it.
Recommends:     gnome-settings-daemon
Recommends:     python3-pyyaml
Recommends:     NetworkManager
Recommends:     upower
# bingux-statusd reads the sound server's state with pactl.
Recommends:     pulseaudio-utils
Recommends:     zenity
# The shell opens Settings from the Control Centre, the dock and the desktop
# menu, and Files is the folder handler it expects. Weak dependencies install
# by default and a minimal install can drop them.
Recommends:     bingux-settings = %{version}-%{release}
Recommends:     bingux-files = %{version}-%{release}
# The compositor and session. The shell also runs on other layer-shell
# compositors, so this is not a hard requirement.
Recommends:     gnoblin

%description
Bingux is a Wayland desktop shell. It provides a top bar, dock, search,
notifications, media controls and an optional sidebar. It is a separate
process from the Gnoblin compositor and can also run on other compositors
that provide the layer-shell protocols.

This package holds the shell, the search and status helper services and the
user units that start them. Settings, Files and the sound effects engine are
separate packages.

%package config
Summary:        Configuration command for the Bingux desktop
License:        GPL-3.0-or-later AND MIT AND Unicode-3.0 AND Zlib

%description config
bingux-config reads, changes and checks the Bingux configuration file in the
user's configuration folder. The shell, the search daemon and Settings use it,
and it can be run by hand. Every way of changing the file goes through the same
validation.

%package settings
Summary:        Settings application for the Bingux desktop
License:        Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib
Requires:       bingux-config = %{version}-%{release}
# Freya draws with Skia on OpenGL and loads the Wayland and keyboard libraries
# at run time, so rpm does not find them on its own.
Requires:       fontconfig
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       mesa-libEGL
Requires:       mesa-dri-drivers
Requires:       hicolor-icon-theme
Requires:       shared-mime-info
Requires:       xdg-utils
# gsettings and gio: appearance, default applications, input settings.
Requires:       dconf
Requires:       /usr/bin/gio
Requires:       /usr/bin/gsettings
Requires:       gsettings-desktop-schemas
# timedatectl and hostnamectl.
Requires:       systemd
# Each page reports a readable error when its backend is missing, so these are
# recommendations and a minimal system can leave them out.
Recommends:     NetworkManager
Recommends:     NetworkManager-wifi
Recommends:     ModemManager
Recommends:     bluez
Recommends:     wireplumber
Recommends:     pipewire-utils
Recommends:     pulseaudio-utils
Recommends:     power-profiles-daemon
Recommends:     upower
Recommends:     accountsservice
Recommends:     udisks2
Recommends:     wireguard-tools
Recommends:     ImageMagick
Recommends:     adwaita-sans-fonts
Recommends:     gnome-settings-daemon
Recommends:     bingux-audio-effects = %{version}-%{release}
Suggests:       bingux-files = %{version}-%{release}

%description settings
Settings is the Bingux desktop's settings application: network, Bluetooth,
sound and sound effects, displays, appearance, the dock, top bar and sidebar,
power, date and time, keyboard, mouse, accounts and default applications. It
opens the page named in a bingux-settings:// address, so links and the shell's
shortcuts land on the right page.

%package files
Summary:        File manager for the Bingux desktop
License:        Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib
Requires:       fontconfig
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       mesa-libEGL
Requires:       mesa-dri-drivers
Requires:       hicolor-icon-theme
Requires:       shared-mime-info
Requires:       xdg-utils
Requires:       /usr/bin/gio
Recommends:     adwaita-sans-fonts
Recommends:     udisks2
Recommends:     gvfs
Recommends:     tar
Recommends:     util-linux
Recommends:     eject
Recommends:     unzip
# Films, songs and PDFs preview through this helper; without it the preview
# shows the file's card and Open with.
Recommends:     bingux-preview-host = %{version}-%{release}
Suggests:       bingux-settings = %{version}-%{release}

%description files
Files is the Bingux desktop's file manager. It has tabs, a places sidebar,
list and grid views, previews, search and file operations with undo. It opens
folders and bingux-files:// addresses.

%package preview-host
Summary:        Helper process that plays media and renders PDFs for Bingux previews
License:        GPL-3.0-or-later AND MIT AND Unicode-3.0
Requires:       gstreamer1-plugins-base
Requires:       gstreamer1-plugins-good
Requires:       poppler-glib
# More decoders: H.264 and the rest. Without them a film that needs one shows
# "This computer has no decoder for ..." and Open with.
Recommends:     gstreamer1-plugin-libav
Recommends:     gstreamer1-plugins-bad-free
Recommends:     gstreamer1-plugin-openh264
Recommends:     pipewire-gstreamer

%description preview-host
bingux-preview-host plays audio and video (GStreamer) and renders PDF pages
(poppler) for the Files preview. It is a separate program so that a damaged
file crashes it and not the file manager, and so that Files itself links
neither library. Files finds it beside its own program or on the PATH.

%package audio-effects
Summary:        Sound effects for the Bingux desktop
License:        GPL-3.0-or-later AND MIT AND Unicode-3.0
Requires:       pipewire >= 1.0
Requires:       systemd
Recommends:     pipewire-utils
Recommends:     wireplumber
Suggests:       lsp-plugins-ladspa

%description audio-effects
bingux-effects puts a chain of plugins between applications and the speakers,
or between a microphone and applications, using PipeWire's filter-chain. The
package includes the Bingux DSP plugin library (compressor, limiter, gate,
reverb, bass enhancer and more) in LADSPA format, which any LADSPA host can
also load. Settings edits the presets and turns the effects on.

%package taxonomy
Summary:        Reference window for the Bingux UI components
License:        Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib
Requires:       fontconfig
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       mesa-libEGL
Requires:       mesa-dri-drivers
Requires:       hicolor-icon-theme
Recommends:     adwaita-sans-fonts

%description taxonomy
Taxonomy shows every component, icon and design token of the Bingux UI
library in a running window. It is a development tool for people who build
Bingux applications; ordinary desktops do not need it.

%prep
%autosetup

%build
export RUSTFLAGS="%{bingux_rustflags}"
%make_build BUILD_DIR=build JOBS=%{?_smp_build_ncpus}%{!?_smp_build_ncpus:2} LIBDIR_NAME=%{_lib}
%make_build licenses BUILD_DIR=build

%install
%make_install PREFIX=%{_prefix} DESTDIR=%{buildroot} BUILD_DIR=build QMLDIR=%{_libdir}/bingux/qml LADSPADIR=%{_libdir}/ladspa
# The scripts name /usr/bin/env; Fedora wants the interpreter itself.
%py3_shebang_fix %{buildroot}%{_libexecdir}/bingux/shell %{buildroot}%{_libexecdir}/bingux/binguxctl.py
# The bundled Adwaita set holds many identical icons; store each once.
hardlink -c %{buildroot}%{_datadir}/bingux/icons

for unit in bingux.target bingux.service bingux-searchd.service bingux-statusd.service bingux-search-ui.service bingux-switcher-ui.service bingux-capture-ui.service bingux-emoji-ui.service bingux-lock.service bingux-greeter-export.service bingux-greeter-export.path bingux-bsm-publish.service bingux-bsm-publish.path xdg-desktop-portal-bingux.service bingux-global-shortcuts.service; do
    sed -e 's|/usr/lib64|%{_libdir}|g' packaging/systemd/$unit > %{buildroot}%{_userunitdir}/$unit
done
install -Dpm 0644 packaging/pam/bingux-lock %{buildroot}%{_sysconfdir}/pam.d/bingux-lock

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/bingux-settings.desktop
desktop-file-validate %{buildroot}%{_datadir}/applications/bingux-files.desktop
desktop-file-validate %{buildroot}%{_datadir}/applications/bingux-taxonomy.desktop
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/dev.drewett.bingux.Settings.metainfo.xml
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/dev.drewett.bingux.Files.metainfo.xml
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/dev.drewett.bingux.Taxonomy.metainfo.xml

# The desktop database, the MIME database and the icon cache are updated by
# file triggers in desktop-file-utils, shared-mime-info and gtk-update-icon-cache,
# so none of these packages needs a scriptlet for its desktop entry, scheme
# handler or icon.

%post
%systemd_user_post bingux.target

%preun
%systemd_user_preun bingux.target bingux-searchd.service bingux-statusd.service

%postun
%systemd_user_postun_with_restart bingux.target bingux-searchd.service bingux-statusd.service

%preun audio-effects
# The effects units are enabled per user from Settings. On removal, stop the
# running instances; an upgrade restarts them.
%systemd_user_preun bingux-effects@output.service bingux-effects@input.service

%postun audio-effects
%systemd_user_postun_with_restart bingux-effects@output.service bingux-effects@input.service

%files
%license COPYING
%license %{_datadir}/licenses/bingux/adwaita-icon-theme/COPYING
%license %{_datadir}/licenses/bingux/adwaita-icon-theme/COPYING_CCBYSA3
%license %{_datadir}/licenses/bingux/adwaita-icon-theme/COPYING_LGPL
%license packages/bingux-shell/quickshell/LICENSE
%license build/licenses/bingux-searchd.LICENSE.dependencies
%license build/licenses/bingux-statusd.LICENSE.dependencies
%doc README.md docs/extensions.md docs/locking.md docs/login-screen.md
%{_bindir}/bingux-shell
%{_bindir}/bingux
%{_bindir}/bingux-capture-ui
%{_bindir}/bingux-lock
%{_bindir}/bingux-emoji-ui
%{_bindir}/bingux-greeter
%{_bindir}/bingux-greeter-session
%{_bindir}/bingux-bsm-greeter
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
%{_bindir}/bingux-helper
%{_libdir}/bingux/
%{_libexecdir}/bingux/
%dir %{_datadir}/bingux
%dir %{_datadir}/bingux/search-apps
%{_datadir}/bingux/gnoblin/
%{_datadir}/bingux/greeter/
%{_datadir}/bingux/icons/
%{_datadir}/bingux/search.json
%{_datadir}/xdg-desktop-portal/portals/bingux.portal
%{_datadir}/dbus-1/services/org.freedesktop.impl.portal.desktop.bingux.service
%{_datadir}/dbus-1/services/org.gnoblin.Shell.GlobalShortcutsProvider.service
%dir %{_datadir}/gnoblin
%dir %{_datadir}/gnoblin/conf.d
%dir %{_datadir}/gnoblin/scripts
%{_datadir}/gnoblin/conf.d/bingux.lua
%{_datadir}/gnoblin/scripts/bingux-text-input.js
%{_userunitdir}/bingux.target
%{_userunitdir}/bingux.service
%{_userunitdir}/bingux-capture-ui.service
%{_userunitdir}/bingux-emoji-ui.service
%{_userunitdir}/bingux-lock.service
%{_userunitdir}/bingux-search-ui.service
%{_userunitdir}/bingux-searchd.service
%{_userunitdir}/bingux-statusd.service
%{_userunitdir}/bingux-switcher-ui.service
%{_userunitdir}/bingux-greeter-export.service
%{_userunitdir}/bingux-greeter-export.path
%{_userunitdir}/bingux-bsm-publish.service
%{_userunitdir}/bingux-bsm-publish.path
%{_userunitdir}/bingux-global-shortcuts.service
%{_userunitdir}/xdg-desktop-portal-bingux.service
%{_userpresetdir}/90-bingux.preset
%{_datadir}/polkit-1/rules.d/50-bingux-greeter.rules
%{_tmpfilesdir}/bingux-greeter.conf
%config(noreplace) %{_sysconfdir}/pam.d/bingux-lock

%files config
%license COPYING
%license build/licenses/bingux-config.LICENSE.dependencies
%doc docs/configuration.md
%{_bindir}/bingux-config
%{_mandir}/man1/bingux-config.1*

%files settings
%license COPYING
%license build/licenses/bingux-settings.LICENSE.dependencies
%doc docs/settings.md
%{_bindir}/bingux-settings
%{_datadir}/applications/bingux-settings.desktop
%{_datadir}/icons/hicolor/scalable/apps/bingux-settings.svg
%{_metainfodir}/dev.drewett.bingux.Settings.metainfo.xml
%{_mandir}/man1/bingux-settings.1*
%dir %{_datadir}/bingux
%dir %{_datadir}/bingux/search-apps
%{_datadir}/bingux/search-apps/bingux-settings.json

%files files
%license COPYING
%license build/licenses/bingux-files.LICENSE.dependencies
%doc docs/files.md
%{_bindir}/bingux-files
%{_datadir}/applications/bingux-files.desktop
%{_datadir}/icons/hicolor/scalable/apps/bingux-files.svg
%{_metainfodir}/dev.drewett.bingux.Files.metainfo.xml
%{_mandir}/man1/bingux-files.1*

%files preview-host
%license COPYING
%license build/licenses/bingux-preview-host.LICENSE.dependencies
%{_bindir}/bingux-preview-host

%files audio-effects
%license COPYING
%license build/licenses/bingux-effects.LICENSE.dependencies
%doc docs/audio-effects.md
%{_bindir}/bingux-effects
%dir %{_libdir}/ladspa
%{_libdir}/ladspa/libbingux_dsp.so
%{_userunitdir}/bingux-effects@.service
%{_mandir}/man1/bingux-effects.1*

%files taxonomy
%license COPYING
%license build/licenses/bingux-taxonomy.LICENSE.dependencies
%doc docs/bingux-ui.md
%{_bindir}/bingux-taxonomy
%{_datadir}/applications/bingux-taxonomy.desktop
%{_datadir}/icons/hicolor/scalable/apps/bingux-taxonomy.svg
%{_metainfodir}/dev.drewett.bingux.Taxonomy.metainfo.xml
%{_mandir}/man1/bingux-taxonomy.1*

%changelog
* Wed Oct 28 2026 Kieran Drewett <kieran@drewett.dev> - 0.1.0-1
- Add bingux-preview-host, the helper process behind the Files preview of
  films, songs and PDFs; Files recommends it.
- Split the package: Settings, Files, the sound effects engine, the
  configuration command and the Taxonomy reference window are their own
  packages, each with desktop entry, AppStream metainfo, icon and man page.
- Enable bingux.target through a user preset.
- Keep the bundled Adwaita icons under %%{_datadir}/bingux so they no longer
  collide with adwaita-icon-theme.
- Add the first native shell package layout.
