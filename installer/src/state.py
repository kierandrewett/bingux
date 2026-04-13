import json
import os
from dataclasses import dataclass, field, asdict

STATE_PATH = "/tmp/bingux-installer-state.json"
REPO_URL_FILE = "/etc/bingux-installer/repo-url"


@dataclass
class InstallerState:
    # Install type: "fresh" or "repository"
    install_type: str = "fresh"

    # Fresh install config
    hostname: str = "bingux"
    profile: str = "workstation"
    desktop: str = "gnome"
    locale: str = ""
    keymap: str = ""

    # Auth
    gh_authenticated: bool = False

    # Repository
    repo_url: str = ""
    repo_cloned: bool = False
    available_hosts: list = field(default_factory=list)
    selected_host: str = ""

    # Disk
    selected_disk: str = ""
    disk_mode: str = "wipe"

    # Partitions
    efi_partition: str = ""
    root_partition: str = ""
    home_partition: str = ""
    swap_partition: str = ""

    # Encryption
    encrypt_root: bool = False
    encrypt_home: bool = False
    luks_passphrase: str = ""

    # Filesystem
    filesystem: str = "btrfs"

    # User
    username: str = ""
    fullname: str = ""
    password: str = ""

    # Install
    install_log: str = ""

    def save(self):
        data = asdict(self)
        data.pop("password", None)
        data.pop("luks_passphrase", None)
        try:
            with open(STATE_PATH, "w") as f:
                json.dump(data, f, indent=2)
        except OSError:
            pass

    @classmethod
    def load(cls):
        try:
            with open(STATE_PATH) as f:
                data = json.load(f)
            state = cls()
            for k, v in data.items():
                if hasattr(state, k):
                    setattr(state, k, v)
            return state
        except (OSError, json.JSONDecodeError):
            return cls()

    @classmethod
    def has_saved_state(cls):
        return os.path.exists(STATE_PATH)

    @classmethod
    def get_preset_repo_url(cls):
        """Read pre-set repo URL baked into the ISO, if any."""
        try:
            with open(REPO_URL_FILE) as f:
                url = f.read().strip()
                return url if url else None
        except OSError:
            return None
