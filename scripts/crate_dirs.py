"""Where each crate or component lives, by the name the build scripts use.

Code that used to build the path `packages/<name>` asks here instead. Add an entry
when a crate is added, moved or renamed; `scripts/check-layout.py` fails when this
table and the tree disagree.
"""

from pathlib import Path

CRATE_DIRS = {
    "bingux-settings": "apps/settings",
    "bingux-files-app": "apps/files",
    "bingux-taxonomy": "apps/taxonomy",
    "bingux-ui": "crates/ui",
    "bingux-files": "crates/files",
    "bingux-preview-core": "crates/preview-core",
    "bingux-search": "crates/search",
    "bingux-data-usage": "crates/data-usage",
    "bingux-config": "crates/config",
    "bingux-searchd": "services/searchd",
    "bingux-statusd": "services/statusd",
    "bingux-session": "services/session",
    "bingux-helper": "services/helper",
    "bingux-portal": "services/portal",
    "bingux-preview-host": "services/preview-host",
    "bingux-audio-effects": "services/audio-effects",
    "bsm": "bsm/core",
    "bsm-appearance": "bsm/appearance",
    "bsm-compositor": "bsm/compositor",
    "bsm-helper": "bsm/helper",
    "bsm-host": "bsm/host",
    "bsm-manager": "bsm/manager",
    "bsm-proto": "bsm/proto",
    "bingux-effects": "native/effects",
    "bingux-text-layout": "native/text-layout",
    "bingux-wayland-sync": "native/wayland-sync",
    "bingux-audio-meter": "native/audio-meter",
    "bingux-image-clipboard": "native/image-clipboard",
    "bingux-frame": "native/frame",
    "bingux-shell": "shell/runtime",
    "bingux-quickshell": "shell/quickshell-patches",
    "bingux-qmltermwidget": "shell/qmltermwidget",
    "binguxctl": "tools/binguxctl",
    "bingux-inventory": "tools/inventory",
    "vendor": "vendor",
}


def crate_dir(root: Path, name: str) -> Path:
    """The directory of crate or component `name` below the repository `root`."""
    return root / CRATE_DIRS[name]
