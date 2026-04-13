import json
import os
import re
import shutil
import subprocess


def get_token():
    """Try to get an access token from gh or glab CLI."""
    for cmd in [["gh", "auth", "token"], ["glab", "auth", "token"]]:
        try:
            r = subprocess.run(cmd, capture_output=True, text=True, check=True)
            token = r.stdout.strip()
            if token:
                return token
        except (subprocess.CalledProcessError, FileNotFoundError):
            continue
    return ""


def configure_nix_token_from_url(clone_url):
    """Extract host + token from a clone URL and write to nix config."""
    m = re.match(r"https://oauth2:([^@]+)@([^/]+)/", clone_url)
    if not m:
        # Try gh CLI token for github.com
        token = get_token()
        if token:
            _write_nix_token("github.com", token)
        return
    token, host = m.group(1), m.group(2)
    _write_nix_token(host, token)


def _write_nix_token(host, token):
    for nix_dir in [
        os.path.expanduser("~/.config/nix"),
        "/root/.config/nix",
    ]:
        try:
            os.makedirs(nix_dir, exist_ok=True)
            conf = os.path.join(nix_dir, "nix.conf")
            existing = open(conf).read() if os.path.exists(conf) else ""
            if f"{host}=" not in existing:
                with open(conf, "a") as f:
                    f.write(f"access-tokens = {host}={token}\n")
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
