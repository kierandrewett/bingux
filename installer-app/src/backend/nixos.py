import os
import shutil
import subprocess


def generate_config():
    """Run nixos-generate-config --root /mnt."""
    try:
        r = subprocess.run(
            ["nixos-generate-config", "--root", "/mnt"],
            capture_output=True, text=True, check=True,
        )
        return True, r.stdout
    except subprocess.CalledProcessError as e:
        return False, e.stderr


def copy_repo(host, repo_path="/tmp/bingux-os"):
    """Copy cloned repo to /mnt/os and hardware config if applicable."""
    dest = "/mnt/os"
    if os.path.isdir(dest):
        shutil.rmtree(dest)
    shutil.copytree(repo_path, dest)

    hw_config = "/mnt/etc/nixos/hardware-configuration.nix"
    machine_dir = os.path.join(dest, "machines", host)
    if os.path.isdir(machine_dir) and os.path.isfile(hw_config):
        shutil.copy2(hw_config, os.path.join(machine_dir, "hardware-configuration.nix"))

    # Set ownership to first normal user (uid 1000)
    for root, dirs, files in os.walk(dest):
        os.chown(root, 1000, 100)
        for f in files:
            os.chown(os.path.join(root, f), 1000, 100)


def generate_ssh_keys():
    """Generate SSH host keys for sops-nix."""
    key_path = "/mnt/etc/ssh/ssh_host_ed25519_key"
    if os.path.exists(key_path):
        return None
    os.makedirs("/mnt/etc/ssh", exist_ok=True)
    subprocess.run(
        ["ssh-keygen", "-t", "ed25519", "-f", key_path, "-N", "", "-q"],
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
            ["nixos-enter", "--root", "/mnt", "--", "chpasswd"],
            stdin=subprocess.PIPE, capture_output=True, text=True,
        )
        p.communicate(input=f"{username}:{password}\n")
        return p.returncode == 0
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False
