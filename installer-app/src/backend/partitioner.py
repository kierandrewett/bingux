import subprocess


def run(cmd, **kwargs):
    """Run a command and return (success, stdout, stderr)."""
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, check=True, **kwargs)
        return True, r.stdout, r.stderr
    except subprocess.CalledProcessError as e:
        return False, e.stdout, e.stderr


def format_fat32(device):
    return run(["mkfs.fat", "-F", "32", "-n", "EFI", device])


def set_efi_type(device):
    """Set the EFI System Partition GUID on a partition."""
    try:
        out = subprocess.run(
            ["lsblk", "-npo", "PKNAME", device], capture_output=True, text=True, check=True,
        )
        disk = out.stdout.strip().split("\n")[0]
        partnum_path = f"/sys/class/block/{device.split('/')[-1]}/partition"
        with open(partnum_path) as f:
            partnum = f.read().strip()
        if disk and partnum:
            run(["sgdisk", "-t", f"{partnum}:ef00", disk])
    except (subprocess.CalledProcessError, OSError):
        pass


def format_filesystem(device, fstype, label="nixos"):
    if fstype == "btrfs":
        return run(["mkfs.btrfs", "-f", "-L", label, device])
    elif fstype == "ext4":
        return run(["mkfs.ext4", "-F", "-L", label, device])
    elif fstype == "xfs":
        return run(["mkfs.xfs", "-f", "-L", label, device])
    return False, "", f"Unsupported filesystem: {fstype}"


def setup_luks(device, passphrase, name="cryptroot"):
    p = subprocess.Popen(
        ["cryptsetup", "luksFormat", "--type", "luks2", "--batch-mode", device],
        stdin=subprocess.PIPE, capture_output=True, text=True,
    )
    _, err = p.communicate(input=passphrase + "\n")
    if p.returncode != 0:
        return False, "", err

    p2 = subprocess.Popen(
        ["cryptsetup", "open", device, name],
        stdin=subprocess.PIPE, capture_output=True, text=True,
    )
    _, err2 = p2.communicate(input=passphrase + "\n")
    return p2.returncode == 0, "", err2


def setup_swap(device):
    ok, _, _ = run(["mkswap", "-L", "swap", device])
    if ok:
        run(["swapon", device])
    return ok


def setup_btrfs_subvolumes(root_device, has_home_partition):
    """Create btrfs subvolumes and mount them."""
    run(["mount", root_device, "/mnt"])
    run(["btrfs", "subvolume", "create", "/mnt/@"])
    run(["btrfs", "subvolume", "create", "/mnt/@nix"])
    if not has_home_partition:
        run(["btrfs", "subvolume", "create", "/mnt/@home"])
    run(["umount", "/mnt"])

    run(["mount", "-o", "subvol=@,compress=zstd,noatime", root_device, "/mnt"])
    run(["mkdir", "-p", "/mnt/boot", "/mnt/nix", "/mnt/home"])
    run(["mount", "-o", "subvol=@nix,compress=zstd,noatime", root_device, "/mnt/nix"])
    if not has_home_partition:
        run(["mount", "-o", "subvol=@home,compress=zstd,noatime", root_device, "/mnt/home"])


def mount_simple(device, mountpoint="/mnt"):
    run(["mount", device, mountpoint])
    run(["mkdir", "-p", f"{mountpoint}/boot", f"{mountpoint}/home"])


def mount_partition(device, mountpoint):
    run(["mkdir", "-p", mountpoint])
    return run(["mount", device, mountpoint])
