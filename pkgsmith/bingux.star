# The `bingux` package: the whole desktop, for an ordinary Fedora, Debian or Arch system that logs in through
# GDM or another display manager that is already there. It holds the shell and every core service the desktop
# does not work without: the session and lock programs, the search and status daemons, the privileged helper, the
# portal backend, the sound effects engine, the preview helper that Files runs, the configuration command, the
# Quickshell-based shell with its QML plugins and the system integration around them (systemd units and presets,
# the PAM service of the lock screen, and the Gnoblin drop-in that starts the shell).
#
# It holds nothing of Bingux OS: bsm, the login screen, greetd and the boot splash belong to the image built in
# distro/, which has its own packages (distro/README.md). scripts/install-shell.py leaves those files out unless it
# is run with --os-image, so they are never in the prefix this script reads.
#
# Settings, Files and Taxonomy are applications and have their own package (apps.star). They depend on this one.
#
# scripts/build-packages.sh runs this on the prefix that scripts/build-prefix.sh made:
#
#   pkgsmith build -f pkgsmith/bingux.star --prefix prefix --build-arg VERSION=0.1.0
#
# The package is everything in the prefix that no application package owns, so a file the build adds is
# packaged without an edit here. tests/packaging.py checks that the application files are all claimed.

load("common.star", "APP_FILES", "FORMATS", "QT_MINOR", "SYSTEMD_RUNNING", "identity", "own_dirs", "qt_pin")

DESCRIPTION = """The Bingux desktop: shell and session services
Bingux is a Wayland desktop. It provides a top bar, dock, search, notifications, media controls and an optional
sidebar for the Gnoblin compositor, and the services behind them: the search and status daemons, the portal backend,
the lock screen, the privileged helper, the configuration command, the sound effects engine and the preview helper
that Files runs. Choose the Gnoblin session in your login screen to start it.

Settings, Files and Taxonomy are separate packages."""

# The packages that this one replaces. Until 0.1.0 the desktop was split into these packages; installing
# `bingux` upgrades them.
OLD_NAMES = ["bingux-config", "bingux-audio-effects", "bingux-preview-host"]

DEPENDS = {
    "rpm": [
        # The compositor and the portal backend that belongs to it, published from repo.gnoblin.org.
        "gnoblin",
        "xdg-desktop-portal-gnoblin",
        "xdg-desktop-portal",
        "systemd",
        "dbus",
        "polkit",
        "pam",
        # The pointer and the icons: Gnoblin leaves both to the shell.
        "adwaita-cursor-theme",
        "adwaita-icon-theme",
        "wireplumber",
        # The sound effects engine hosts LV2 plugins through PipeWire's filter chain.
        "pipewire >= 1.4",
        "pipewire-module-filter-chain-lv2",
        "lilv-libs",
        # The shell: Qt's QML runtime, which the shell links against; Quickshell itself is inside the binary.
        "qt6-qtdeclarative",
        "qt6-qtwayland",
        "qt6-qtsvg",
        "qt6-qtmultimedia",
        # Python helpers of the shell.
        "python3",
        "python3-cairo",
        "python3-gobject",
        "python3-pillow",
        "gtk3",
        "gtk4",
        "libadwaita",
        "/usr/bin/ffmpeg",
        "wl-clipboard",
        "xdg-utils",
        # Desktop-wide settings (accent, colour scheme, background, idle delay) live in GSettings;
        # dconf keeps them across logins.
        "dconf",
        "/usr/bin/gsettings",
        # Preview helper: it plays media with GStreamer and renders PDFs with poppler.
        "gstreamer1-plugins-base",
        "gstreamer1-plugins-good",
        "poppler-glib",
        # Libraries of the programs; scripts/check-package-libs.sh finds a missing one.
        "fontconfig",
        "mesa-libEGL",
        "mesa-dri-drivers",
        "mesa-libgbm",
        "libdrm",
        "libxcb",
        "libxkbcommon",
        "libarchive",
        "jemalloc",
        "pipewire-libs",
        "polkit-libs",
    ],
    "deb": [
        "gnoblin",
        "xdg-desktop-portal-gnoblin",
        "xdg-desktop-portal",
        "systemd",
        "dbus",
        "polkitd",
        "libpam-modules",
        "adwaita-icon-theme",
        "wireplumber",
        "pipewire (>= 1.4)",
        "libspa-0.2-modules (>= 1.4)",
        "liblilv-0-0",
        "qml6-module-qtcore",
        "qml6-module-qtmultimedia",
        "qml6-module-qtqml-models",
        "qml6-module-qtqml-workerscript",
        "qml6-module-qtquick",
        "qml6-module-qtquick-controls",
        "qml6-module-qtquick-dialogs",
        "qml6-module-qtquick-effects",
        "qml6-module-qtquick-layouts",
        "qml6-module-qtquick-shapes",
        "qml6-module-qtquick-window",
        "qt6-svg-plugins",
        "qt6-wayland",
        "python3",
        "python3-cairo",
        "python3-gi",
        "python3-gi-cairo",
        "python3-pil",
        "gir1.2-gtk-3.0",
        "gir1.2-gtk-4.0",
        "ffmpeg",
        "wl-clipboard",
        "xdg-utils",
        "dconf-gsettings-backend | gsettings-backend",
        "libglib2.0-bin",
        "gstreamer1.0-plugins-base",
        "gstreamer1.0-plugins-good",
        "fontconfig",
        "libegl1",
        "libgl1-mesa-dri",
        "libgbm1",
        "libdrm2",
        "libxcb1",
        "libxkbcommon0",
        "libarchive13t64",
        "libjemalloc2",
        "libpipewire-0.3-0t64",
        "libpolkit-agent-1-0",
        "libpoppler-glib8t64",
        "libqt6widgets6",
        "libadwaita-1-0",
    ],
    "archlinux": [
        "gnoblin",
        "xdg-desktop-portal-gnoblin",
        "xdg-desktop-portal",
        "systemd",
        "dbus",
        "polkit",
        "pam",
        "adwaita-cursors",
        "adwaita-icon-theme",
        "wireplumber",
        "pipewire",
        "pipewire-audio",
        "lilv",
        "qt6-declarative",
        "qt6-wayland",
        "qt6-svg",
        "qt6-multimedia",
        "python",
        "python-cairo",
        "python-gobject",
        "python-pillow",
        "gtk3",
        "gtk4",
        "libadwaita",
        "ffmpeg",
        "wl-clipboard",
        "xdg-utils",
        "dconf",
        "glib2",
        "gst-plugins-base",
        "gst-plugins-good",
        "poppler-glib",
        "fontconfig",
        "libglvnd",
        "mesa",
        "libdrm",
        "libxcb",
        "libxkbcommon",
        "libarchive",
        "jemalloc",
        "libpulse",
    ],
}

# Installed by default where the format has weak dependencies (rpm, deb). Arch lists them as optional
# dependencies with the reason, which pacman prints.
RECOMMENDS = {
    "rpm": [
        # The shell opens Settings from the Control Centre, the dock and the desktop menu, and Files is
        # the folder handler it expects. A minimal install can drop them.
        "bingux-settings",
        "bingux-files",
        # The sound effects engine runs the published plugins that Easy Effects uses.
        "lsp-plugins-lv2",
        "lv2-calf-plugins",
        "lv2-zam-plugins",
        "lv2-mdala-plugins",
        "lv2-x42-plugins",
        "ladspa-rubberband-plugins",
        "pipewire-utils",
        # Film decoders for the Files preview.
        "gstreamer1-plugin-libav",
        "gstreamer1-plugins-bad-free",
        "gstreamer1-plugin-openh264",
        "pipewire-gstreamer",
        "gnome-settings-daemon",
        "python3-pyyaml",
        "NetworkManager",
        "upower",
        "pulseaudio-utils",
        "zenity",
    ],
    "deb": [
        "bingux-settings",
        "bingux-files",
        "lsp-plugins-lv2",
        "calf-plugins",
        "zam-plugins",
        "mda-lv2",
        "x42-plugins",
        "rubberband-ladspa",
        "pipewire-bin",
        "gstreamer1.0-libav",
        "gstreamer1.0-pipewire",
        "gstreamer1.0-plugins-bad",
        "gnome-settings-daemon",
        "python3-yaml",
        "network-manager",
        "upower",
        "pulseaudio-utils",
        "zenity",
    ],
    "archlinux": [
        "bingux-settings: Settings, opened from the Control Centre and the dock",
        "bingux-files: file manager",
        "lsp-plugins: sound effects",
        "calf: sound effects: bass enhancer, exciter, crusher, de-esser, reverb and stereo tools",
        "zam-plugins: sound effects: maximizer",
        "mda.lv2: sound effects: bass loudness",
        "x42-plugins: sound effects: autotune",
        "rubberband: sound effects: pitch",
        "gst-libav: more video and audio formats in previews",
        "gst-plugins-bad: more video and audio formats in previews",
        "gst-plugin-pipewire: audio output of previews through PipeWire",
        "gnome-settings-daemon: night light",
        "networkmanager: network status and Wi-Fi",
        "upower: battery",
        "python-yaml: YAML frontmatter in document previews",
        "zenity: file dialogs for helper scripts",
    ],
}

EFFECTS = "bingux-effects@output.service bingux-effects@input.service"
UNITS = "bingux.target bingux-searchd.service bingux-statusd.service " + EFFECTS

SCRIPTS = {
    # The bodies of %systemd_user_post, %systemd_user_preun and %systemd_user_postun_with_restart of Fedora 43,
    # which enable bingux.target through the user preset on the first install and restart the daemons on upgrade.
    "rpm": {
        "post_install": """if [ $1 -eq 1 ]; then
    if [ -x /usr/lib/systemd/systemd-update-helper ]; then
        /usr/lib/systemd/systemd-update-helper install-user-units bingux.target || :
    else
        systemctl --global preset bingux.target >/dev/null 2>&1 || :
    fi
fi
""",
        "pre_remove": """if [ $1 -eq 0 ] && [ -x /usr/lib/systemd/systemd-update-helper ]; then
    /usr/lib/systemd/systemd-update-helper remove-user-units """ + UNITS + """ || :
fi
""",
        "post_remove": """if [ $1 -ge 1 ] && [ -x /usr/lib/systemd/systemd-update-helper ]; then
    /usr/lib/systemd/systemd-update-helper mark-restart-user-units """ + UNITS + """ || :
fi
""",
    },
    # dpkg passes configure after an install or an upgrade and remove on removal; the preset file enables
    # bingux.target for every user.
    "deb": {
        "post_install": """if [ "$1" = configure ] && """ + SYSTEMD_RUNNING + """; then
    systemctl --global preset bingux.target >/dev/null 2>&1 || true
fi
""",
        "pre_remove": """if [ "$1" = remove ] && """ + SYSTEMD_RUNNING + """; then
    systemctl --global disable bingux.target >/dev/null 2>&1 || true
fi
""",
    },
    # pacman runs the install script for post_install and post_upgrade, and pre_remove only on removal.
    "archlinux": {
        "post_install": """systemctl --global preset bingux.target >/dev/null 2>&1 || true
""",
        "pre_remove": """systemctl --global disable bingux.target >/dev/null 2>&1 || true
""",
    },
}

# Configuration files: the package manager keeps the changes of the administrator.
CONFIG_FILES = ["/etc/pam.d/bingux-lock", "/etc/pam.d/bingux-lock-fingerprint"]

# The folders of the package. /usr/share/bingux/search-apps is shared with Settings, which ships a file in it.
OWN_DIRS = own_dirs(
    "/usr/lib/bingux",
    "/usr/lib64/bingux",
    "/usr/libexec/bingux",
    "/usr/share/bingux",
    "/usr/share/doc/bingux",
    "/usr/share/licenses/bingux",
)

# Every file of an application package belongs to that package; the rest of the prefix is this one.
exclude(APP_FILES)

for fmt in FORMATS:
    fields = identity(fmt, "bingux", DESCRIPTION, "bingux")
    if fmt == "tar":
        package(fmt, **fields)
        continue
    # The shell and its plugins link the private API of the Qt they were built with.
    depends = DEPENDS[fmt] + (qt_pin(fmt) if QT_MINOR else [])
    old = OLD_NAMES
    package(
        fmt,
        depends = depends,
        recommends = RECOMMENDS[fmt],
        # rpm: Obsoletes. deb: Replaces and Conflicts. archlinux: replaces and conflicts. Each also Provides the name, so a package
        # of an older release that still depends on it stays satisfied.
        replaces = old,
        conflicts = [] if fmt == "rpm" else old,
        provides = old,
        config_files = CONFIG_FILES,
        own_dirs = OWN_DIRS,
        scripts = SCRIPTS[fmt],
        options = {"section": "x11"} if fmt == "deb" else {},
        **fields
    )
