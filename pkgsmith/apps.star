# The application packages of Bingux: Settings, Files, Preview, Media, Weather and Taxonomy. Each is its own package, holds the files
# listed for it in common.star (APPS) and depends on `bingux`, which has the core services the apps talk to:
# the configuration command, the sound effects engine, the preview helper and the search daemon.
#
# scripts/build-packages.sh runs this on the prefix that scripts/build-prefix.sh made:
#
#   pkgsmith build -f pkgsmith/apps.star --prefix prefix --build-arg VERSION=0.1.0

load("common.star", "APPS", "FORMATS", "app_files", "identity", "own_dirs", "pin_bingux")

DESCRIPTIONS = {
    "bingux-settings": """Settings application for the Bingux desktop
Settings is the Bingux desktop's settings application: network, Bluetooth, sound and sound effects, displays,
appearance, the dock, top bar and sidebar, power, date and time, keyboard, mouse, accounts and default applications.
It opens the page named in a bingux-settings:// address, so links and the shell's shortcuts land on the right page.""",
    "bingux-calculator": """Calculator for the Bingux desktop
Calculator has basic, scientific, programmer and conversion modes. Arithmetic is exact, results show as you type, and
the history is kept. It converts units and currencies, with exchange rates cached for use without a network.""",
    "bingux-files": """File manager for the Bingux desktop
Files is the Bingux desktop's file manager. It has tabs, a places sidebar, list and grid views, previews, search and
file operations with undo. It opens folders and bingux-files:// addresses.""",
    "bingux-weather": """Weather for the Bingux desktop
Weather shows the conditions now, the next 24 hours and ten days for where the computer is and for the cities you add:
UV index, wind, sunrise and sunset, the moon, humidity, pressure, visibility and the air quality. Data is by Open-Meteo.com
(CC BY 4.0); it needs no account and no key.""",
    "bingux-text-editor": """Text and code editor for the Bingux desktop
Text Editor edits plain text and code in tabs, with colours for about 200 languages, find and replace, any encoding
and line ending, a Markdown preview and drafts that keep unsaved text across restarts.""",
    "bingux-preview": """Picture and document viewer for the Bingux desktop
Preview shows pictures and PDF documents, with the other files of the folder one arrow key away. It zooms, turns and
crops pictures, plays animated GIFs, runs a slideshow and shows a document's pages and outline. HEIF, AVIF, TIFF,
HDR, OpenEXR and camera pictures and PDFs are read by the preview helper in the bingux package.""",
    "bingux-taxonomy": """Reference window for the Bingux UI components
Taxonomy shows every component, icon and design token of the Bingux UI library in a running window. It is a
development tool for people who build Bingux applications; ordinary desktops do not need it.""",
    "bingux-media": """Music and video player for the Bingux desktop
Media plays the music and videos in the Music and Videos folders with nothing to set up: albums, artists, songs and
videos, a queue, subtitles, chapters, resume, a mini player and the desktop's media controls (MPRIS). Playback is
mpv's, through libmpv.""",
}

# Freya draws with Skia on OpenGL and loads the Wayland and keyboard libraries at run time, so nothing finds them
# from the program. Every application lists them.
GRAPHICS = {
    "rpm": ["fontconfig", "libwayland-client", "libwayland-egl", "libxkbcommon", "mesa-libEGL", "mesa-dri-drivers", "hicolor-icon-theme"],
    "deb": ["fontconfig", "libegl1", "libgl1-mesa-dri", "libwayland-client0", "libwayland-egl1", "libxkbcommon0", "hicolor-icon-theme"],
    "archlinux": ["fontconfig", "freetype2", "libglvnd", "mesa", "libxkbcommon", "wayland", "hicolor-icon-theme"],
}

DEPENDS = {
    "bingux-settings": {
        "rpm": ["shared-mime-info", "xdg-utils", "dconf", "/usr/bin/gio", "/usr/bin/gsettings", "gsettings-desktop-schemas", "systemd"],
        "deb": ["shared-mime-info", "xdg-utils", "dconf-gsettings-backend | gsettings-backend", "libglib2.0-bin", "gsettings-desktop-schemas", "systemd"],
        "archlinux": ["shared-mime-info", "xdg-utils", "glib2", "dconf", "gsettings-desktop-schemas", "systemd"],
    },
    "bingux-calculator": {"rpm": [], "deb": [], "archlinux": []},
    "bingux-files": {
        "rpm": ["shared-mime-info", "xdg-utils", "/usr/bin/gio"],
        "deb": ["shared-mime-info", "xdg-utils", "libglib2.0-bin"],
        "archlinux": ["shared-mime-info", "xdg-utils", "glib2"],
    },
    "bingux-weather": {"rpm": ["xdg-utils"], "deb": ["xdg-utils"], "archlinux": ["xdg-utils"]},
    "bingux-text-editor": {
        "rpm": ["shared-mime-info", "xdg-utils"],
        "deb": ["shared-mime-info", "xdg-utils"],
        "archlinux": ["shared-mime-info", "xdg-utils"],
    },
    "bingux-preview": {
        "rpm": ["shared-mime-info", "xdg-utils"],
        "deb": ["shared-mime-info", "xdg-utils"],
        "archlinux": ["shared-mime-info", "xdg-utils"],
    },
    "bingux-taxonomy": {"rpm": [], "deb": [], "archlinux": []},
    # libmpv plays everything; SQLite is built in.
    "bingux-media": {
        "rpm": ["mpv-libs", "shared-mime-info", "xdg-utils"],
        "deb": ["libmpv2", "shared-mime-info", "xdg-utils"],
        "archlinux": ["mpv", "shared-mime-info", "xdg-utils"],
    },
}

# Each page of Settings reports a readable error when its backend is missing, so these are recommendations and
# a minimal system can leave them out.
RECOMMENDS = {
    "bingux-settings": {
        "rpm": [
            "NetworkManager", "NetworkManager-wifi", "ModemManager", "bluez", "wireplumber", "pipewire-utils",
            "pulseaudio-utils", "power-profiles-daemon", "upower", "accountsservice", "udisks2", "wireguard-tools",
            "ImageMagick", "adwaita-sans-fonts", "gnome-settings-daemon",
        ],
        "deb": [
            "network-manager", "modemmanager", "bluez", "wireplumber", "pipewire-bin", "pulseaudio-utils",
            "power-profiles-daemon", "upower", "accountsservice", "udisks2", "wireguard-tools", "imagemagick",
            "gnome-settings-daemon",
        ],
        "archlinux": [
            "networkmanager: Network page",
            "modemmanager: mobile network",
            "bluez: Bluetooth page",
            "bluez-utils: Bluetooth page",
            "wireplumber: sound devices and volumes",
            "pipewire: sound effects and application volume",
            "libpulse: sound devices (pactl)",
            "power-profiles-daemon: power profiles",
            "upower: battery and power",
            "accountsservice: Users page",
            "udisks2: disk usage",
            "wireguard-tools: WireGuard VPN",
            "imagemagick: wallpaper previews",
            "adwaita-fonts: the Adwaita Sans interface font",
            "gnome-settings-daemon: night light and input settings",
        ],
    },
    "bingux-calculator": {
        "rpm": ["adwaita-sans-fonts"],
        "deb": [],
        "archlinux": ["adwaita-fonts: the Adwaita Sans interface font"],
    },
    "bingux-files": {
        "rpm": ["adwaita-sans-fonts", "udisks2", "gvfs", "tar", "util-linux", "eject", "unzip"],
        "deb": ["eject", "gvfs", "tar", "udisks2", "unzip", "util-linux"],
        "archlinux": [
            "adwaita-fonts: the Adwaita Sans interface font",
            "udisks2: mount and unmount drives",
            "gvfs: more places and trash integration",
            "unzip: open zip archives",
        ],
    },
    # The location service answers "where am I" when there is one; without it the window uses the network address.
    "bingux-weather": {
        "rpm": ["adwaita-sans-fonts", "geoclue2", "xdg-desktop-portal"],
        "deb": ["geoclue-2.0", "xdg-desktop-portal"],
        "archlinux": [
            "adwaita-fonts: the Adwaita Sans interface font",
            "geoclue: the position from the location service",
            "xdg-desktop-portal: the portal that asks for the position",
        ],
    },
    "bingux-text-editor": {
        "rpm": ["adwaita-sans-fonts", "adwaita-mono-fonts"],
        "deb": [],
        "archlinux": ["adwaita-fonts: the Adwaita Sans and Mono fonts"],
    },
    "bingux-preview": {
        "rpm": ["adwaita-sans-fonts"],
        "deb": [],
        "archlinux": ["adwaita-fonts: the Adwaita Sans interface font"],
    },
    "bingux-taxonomy": {
        "rpm": ["adwaita-sans-fonts"],
        "deb": [],
        "archlinux": ["adwaita-fonts: the Adwaita Sans interface font"],
    },
    "bingux-media": {
        "rpm": ["adwaita-sans-fonts", "pipewire-pulseaudio"],
        "deb": ["pipewire-pulse"],
        "archlinux": ["adwaita-fonts: the Adwaita Sans interface font", "pipewire-pulse: sound output"],
    },
}

# The applications that name each other, as suggestions.
SUGGESTS = {
    "bingux-settings": {"rpm": ["bingux-files"], "deb": ["bingux-files"], "archlinux": []},
    "bingux-calculator": {"rpm": [], "deb": [], "archlinux": []},
    "bingux-files": {"rpm": ["bingux-settings"], "deb": ["bingux-settings"], "archlinux": []},
    "bingux-weather": {"rpm": [], "deb": [], "archlinux": []},
    "bingux-text-editor": {"rpm": ["bingux-files"], "deb": ["bingux-files"], "archlinux": []},
    "bingux-preview": {"rpm": ["bingux-files"], "deb": ["bingux-files"], "archlinux": []},
    "bingux-taxonomy": {"rpm": [], "deb": [], "archlinux": []},
    "bingux-media": {"rpm": ["bingux-files"], "deb": ["bingux-files"], "archlinux": []},
}

for name in APPS:
    # A package holds its own files and nothing else; reset() forgets the rules of the one before.
    reset()
    include(app_files(name))
    for fmt in FORMATS:
        fields = identity(fmt, name, DESCRIPTIONS[name], name)
        if fmt == "tar":
            package(fmt, **fields)
            continue
        package(
            fmt,
            depends = [pin_bingux(fmt)] + GRAPHICS[fmt] + DEPENDS[name][fmt],
            recommends = RECOMMENDS[name][fmt],
            suggests = SUGGESTS[name][fmt],
            own_dirs = own_dirs("/usr/share/doc/" + name, "/usr/share/licenses/" + name) + (["/usr/share/bingux/search-apps"] if name == "bingux-settings" else []),
            options = {"section": "x11"} if fmt == "deb" else {},
            **fields
        )
