import json
import os
import shutil
import subprocess


def is_authenticated():
    try:
        r = subprocess.run(["gh", "auth", "status"], capture_output=True, text=True)
        return r.returncode == 0
    except FileNotFoundError:
        return False


def login():
    """Launch gh auth login in a terminal window.

    gh needs an interactive TTY for the device code flow, so we open
    a terminal. The user completes auth there, then clicks Check Status.
    """
    subprocess.Popen([
        "gnome-terminal", "--", "bash", "-c",
        "gh auth login -p https -w; echo; echo 'Done — you can close this window.'; read",
    ])


def get_token():
    try:
        r = subprocess.run(["gh", "auth", "token"], capture_output=True, text=True, check=True)
        return r.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""


def configure_nix_token():
    """Write GitHub token to nix config for flake access."""
    token = get_token()
    if not token:
        return
    # Write to both user and root nix config
    for nix_dir in [
        os.path.expanduser("~/.config/nix"),
        "/root/.config/nix",
    ]:
        try:
            os.makedirs(nix_dir, exist_ok=True)
            conf = os.path.join(nix_dir, "nix.conf")
            existing = open(conf).read() if os.path.exists(conf) else ""
            if "github.com=" not in existing:
                with open(conf, "a") as f:
                    f.write(f"access-tokens = github.com={token}\n")
        except OSError:
            pass


def clone_repo(url, dest="/tmp/bingux-os"):
    """Clone a git repository. Returns (success, error_message)."""
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    try:
        subprocess.run(["git", "clone", url, dest], check=True, capture_output=True, text=True)
        return True, ""
    except subprocess.CalledProcessError as e:
        return False, e.stderr


def enumerate_hosts(repo_path="/tmp/bingux-os"):
    """List NixOS host names from a flake."""
    try:
        r = subprocess.run(
            ["nix", "flake", "show", "--json", repo_path],
            capture_output=True, text=True, check=True,
        )
        data = json.loads(r.stdout)
        return sorted(data.get("nixosConfigurations", {}).keys())
    except (subprocess.CalledProcessError, json.JSONDecodeError):
        return []
