import glob
import json
import os
import shutil
import subprocess


def generate_config():
    """Run nixos-generate-config --root /mnt."""
    try:
        r = subprocess.run(
            ["sudo", "nixos-generate-config", "--root", "/mnt"],
            capture_output=True, text=True, check=True,
        )
        return True, r.stdout
    except subprocess.CalledProcessError as e:
        return False, e.stderr


def _read_hw_config_path(repo_path, host):
    """Read bingux.hardwareConfigPath from the NixOS config.

    This is set via the hardwareConfigPath parameter in mkBinguxHost:

        bingux.lib.mkBinguxHost {
            hostname = "my-host";
            hardwareConfigPath = "machines/my-host";
            ...
        };

    Defaults to "machines/<hostname>".
    """
    try:
        r = subprocess.run(
            ["nix", "eval", "--json",
             f"{repo_path}#nixosConfigurations.{host}.config.bingux.hardwareConfigPath"],
            capture_output=True, text=True,
        )
        if r.returncode == 0:
            path = json.loads(r.stdout)
            if isinstance(path, str) and path:
                return os.path.join(repo_path, path)
    except (json.JSONDecodeError, subprocess.CalledProcessError):
        pass
    return None


def _find_hw_config_dir(repo_path, host):
    """Fallback: search common layouts for where hardware-configuration.nix belongs."""
    candidates = [
        os.path.join(repo_path, "machines", host),
        os.path.join(repo_path, "hosts", host),
        os.path.join(repo_path, host),
        os.path.join(repo_path, "nixos", "machines", host),
        os.path.join(repo_path, "nixos", "hosts", host),
    ]

    for d in candidates:
        if os.path.isdir(d):
            return d

    # Search for an existing hardware-configuration.nix stub anywhere
    for path in glob.glob(os.path.join(repo_path, "**", "hardware-configuration.nix"), recursive=True):
        return os.path.dirname(path)

    return None


def copy_repo(host, repo_path="/tmp/bingux-os", log_callback=None):
    """Copy cloned repo to /mnt/os and place hardware config.

    Resolution order for hardware-configuration.nix placement:
    1. bingux.hardwareConfigPath.<host> from the flake (explicit)
    2. Heuristic search of common directory layouts
    3. Left at /mnt/etc/nixos/ for manual integration
    """
    dest = "/mnt/os"
    subprocess.run(["sudo", "rm", "-rf", dest], capture_output=True)
    subprocess.run(["sudo", "cp", "-a", repo_path, dest], capture_output=True, check=True)

    hw_config = "/mnt/etc/nixos/hardware-configuration.nix"
    if os.path.isfile(hw_config):
        # Try explicit path from flake first
        target_dir = _read_hw_config_path(dest, host)
        if target_dir:
            subprocess.run(["sudo", "mkdir", "-p", target_dir], capture_output=True)
        else:
            target_dir = _find_hw_config_dir(dest, host)

        if target_dir:
            target = os.path.join(target_dir, "hardware-configuration.nix")
            subprocess.run(["sudo", "cp", hw_config, target], capture_output=True)
            if log_callback:
                log_callback(f"Hardware config placed at: {target}\n")
        elif log_callback:
            log_callback(
                "Hardware config generated at /mnt/etc/nixos/hardware-configuration.nix\n"
                "Integrate it into your flake if needed.\n"
            )

    # Set ownership to first normal user (uid 1000)
    subprocess.run(["sudo", "chown", "-R", "1000:100", dest], capture_output=True)


def generate_ssh_keys():
    """Generate SSH host keys for sops-nix."""
    key_path = "/mnt/etc/ssh/ssh_host_ed25519_key"
    if os.path.exists(key_path):
        return None
    subprocess.run(["sudo", "mkdir", "-p", "/mnt/etc/ssh"], capture_output=True)
    subprocess.run(
        ["sudo", "ssh-keygen", "-t", "ed25519", "-f", key_path, "-N", "", "-q"],
        check=True,
    )
    try:
        r = subprocess.run(
            ["ssh-to-age"],
            stdin=open(key_path + ".pub"),
            capture_output=True, text=True,
        )
        return r.stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def install(host, log_callback=None):
    """Run nixos-install. Calls log_callback(line) for each output line."""
    cmd = [
        "sudo",
        "nixos-install",
        "--no-root-passwd",
        "--root", "/mnt",
        "--flake", f"/mnt/os#{host}",
    ]
    process = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    for line in process.stdout:
        if log_callback:
            log_callback(line)
    process.wait()
    return process.returncode == 0


def set_password(username, password):
    """Set user password in the installed system."""
    try:
        p = subprocess.Popen(
            ["sudo", "nixos-enter", "--root", "/mnt", "--", "chpasswd"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        p.communicate(input=f"{username}:{password}\n")
        return p.returncode == 0
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False
