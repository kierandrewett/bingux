"""Minimal TUI installer for headless/server environments."""

import os
import sys
import subprocess
from state import InstallerState
from backend import github, partitioner, nixos, config_generator
from backend.disks import list_disks, format_size, detect_partitions

BOLD = "\033[1m"
DIM = "\033[2m"
RED = "\033[31m"
GREEN = "\033[32m"
YELLOW = "\033[33m"
BLUE = "\033[34m"
RESET = "\033[0m"


def info(msg):
    print(f"  {BLUE}::{RESET} {msg}")

def success(msg):
    print(f"  {GREEN}\u2713{RESET} {msg}")

def warn(msg):
    print(f"  {YELLOW}!{RESET} {msg}")

def fail(msg):
    print(f"  {RED}\u2717{RESET} {msg}")

def prompt(label, default=""):
    suffix = f" [{BOLD}{default}{RESET}]" if default else ""
    val = input(f"  {BOLD}\u25b8{RESET} {label}{suffix}: ").strip()
    return val or default

def confirm(label):
    return prompt(label, "y/N").lower().startswith("y")


def run_tui():
    print()
    print(f"  {BOLD}{BLUE}Bingux Installer{RESET}  {DIM}(console mode){RESET}")
    print(f"  {DIM}{'─' * 50}{RESET}")
    print()

    state = InstallerState()

    # Locale
    state.locale = prompt("Locale", "en_US.UTF-8")
    print()

    # Install type
    print(f"  {BOLD}Installation type:{RESET}")
    print(f"    {GREEN}1{RESET})  Fresh install")
    print(f"    {YELLOW}2{RESET})  From repository")
    print()
    mode = prompt("Choice", "1")
    state.install_type = "repository" if mode == "2" else "fresh"
    print()

    if state.install_type == "fresh":
        state.hostname = prompt("Hostname", "bingux")
        state.profile = prompt("Profile (workstation/laptop/generic)", "workstation")
        state.desktop = prompt("Desktop (gnome/gnome-default/kde/xfce)", "gnome")
        state.username = prompt("Username")
        if state.username:
            import getpass
            while True:
                p1 = getpass.getpass(f"  {BOLD}\u25b8{RESET} Password: ")
                p2 = getpass.getpass(f"  {BOLD}\u25b8{RESET} Confirm:  ")
                if p1 == p2 and p1:
                    state.password = p1
                    break
                warn("Passwords don't match or are empty.")
        state.selected_host = state.hostname
        print()
    else:
        repo_url = prompt("Repository URL")
        if not repo_url:
            fail("No URL provided.")
            return 1

        info(f"Cloning {repo_url}...")
        ok, err = github.clone_repo(repo_url)
        if not ok:
            fail(f"Clone failed: {err}")
            token = prompt("Access token (blank to skip)")
            if token:
                import re
                authed_url = re.sub(r"^https://", f"https://oauth2:{token}@", repo_url)
                ok, err = github.clone_repo(authed_url)
                if not ok:
                    fail(f"Clone failed: {err}")
                    return 1
                github.configure_nix_token_from_url(authed_url)
            else:
                return 1

        success("Repository cloned.")
        hosts = github.enumerate_hosts()
        if not hosts:
            fail("No nixosConfigurations found.")
            return 1

        print()
        for i, h in enumerate(hosts):
            print(f"    {GREEN}{i+1}{RESET})  {BOLD}{h}{RESET}")
        print()

        if len(hosts) == 1:
            state.selected_host = hosts[0]
            info(f"Selected: {state.selected_host}")
        else:
            idx = int(prompt(f"Select host [1-{len(hosts)}]", "1")) - 1
            state.selected_host = hosts[idx]
        state.repo_url = repo_url
        print()

    # Disk
    print(f"  {BOLD}Available disks:{RESET}")
    disks = list_disks()
    for i, d in enumerate(disks):
        name = d.get("name", "")
        model = d.get("model") or "Unknown"
        size = format_size(d.get("size"))
        print(f"    {GREEN}{i+1}{RESET})  {model}  ({name}, {size})")
    print()
    disk_idx = int(prompt(f"Select disk [1-{len(disks)}]", "1")) - 1
    state.selected_disk = disks[disk_idx]["name"]

    print()
    print(f"  {BOLD}Partitioning mode:{RESET}")
    print(f"    {GREEN}1{RESET})  Erase entire disk")
    print(f"    {YELLOW}2{RESET})  Manual (partition yourself, then assign)")
    print()
    disk_mode = prompt("Choice", "1")
    state.disk_mode = "manual" if disk_mode == "2" else "wipe"

    if state.disk_mode == "manual":
        print()
        info("Partition the disk now. Use fdisk, parted, or another tool.")
        info("When done, press Enter to continue.")
        input(f"  {BOLD}\u25b8{RESET} Press Enter... ")
        print()

        # Show partitions and let user assign
        from backend.disks import list_partitions
        parts = list_partitions(state.selected_disk)
        efi_auto, root_auto, swap_auto = detect_partitions(state.selected_disk)

        print(f"  {BOLD}Partitions on {state.selected_disk}:{RESET}")
        for p in parts:
            name = p.get("name", "")
            size = format_size(p.get("size"))
            fstype = p.get("fstype") or ""
            print(f"    {name}  {size}  {fstype}")
        print()

        state.efi_partition = prompt("EFI partition", efi_auto)
        state.root_partition = prompt("Root partition", root_auto)
        state.home_partition = prompt("Home partition (blank to skip)", "")
        state.swap_partition = prompt("Swap partition (blank to skip)", swap_auto)

    state.filesystem = prompt("Filesystem (btrfs/ext4/xfs)", "btrfs")
    state.encrypt_root = confirm("Encrypt root with LUKS2?")
    if state.encrypt_root:
        import getpass
        while True:
            p1 = getpass.getpass(f"  {BOLD}\u25b8{RESET} LUKS passphrase: ")
            p2 = getpass.getpass(f"  {BOLD}\u25b8{RESET} Confirm:         ")
            if p1 == p2 and p1:
                state.luks_passphrase = p1
                break
            warn("Passphrases don't match.")

    print()
    print(f"  {BOLD}Summary:{RESET}")
    print(f"    Host:       {state.selected_host}")
    if state.disk_mode == "wipe":
        print(f"    Disk:       {state.selected_disk}  (WIPE)")
    else:
        print(f"    EFI:        {state.efi_partition}")
        print(f"    Root:       {state.root_partition}")
        if state.home_partition:
            print(f"    Home:       {state.home_partition}")
        if state.swap_partition:
            print(f"    Swap:       {state.swap_partition}")
    print(f"    Filesystem: {state.filesystem}")
    if state.encrypt_root:
        print(f"    Encryption: LUKS2")
    print()
    if state.disk_mode == "wipe":
        print(f"  {RED}{BOLD}WARNING: This will erase all data on {state.selected_disk}!{RESET}")
    else:
        print(f"  {RED}{BOLD}WARNING: This will format the selected partitions!{RESET}")
    if not confirm("Type 'y' to proceed"):
        warn("Aborted.")
        return 1

    print()

    # Partition
    if state.disk_mode == "wipe":
        info(f"Wiping {state.selected_disk}...")
        ok, _, err = partitioner.wipe_disk(state.selected_disk)
        if not ok:
            fail(f"Partitioning failed: {err}")
            return 1

        disk = state.selected_disk
        sep = "p" if "nvme" in disk or "mmcblk" in disk else ""
        state.efi_partition = f"{disk}{sep}1"
        state.root_partition = f"{disk}{sep}2"

    success(f"EFI: {state.efi_partition}  Root: {state.root_partition}")

    root_dev = state.root_partition
    if state.encrypt_root:
        info("Setting up LUKS...")
        ok, _, err = partitioner.setup_luks(state.root_partition, state.luks_passphrase)
        if not ok:
            fail(f"LUKS failed: {err}")
            return 1
        root_dev = "/dev/mapper/cryptroot"
        success("LUKS ready.")

    info("Formatting EFI...")
    partitioner.format_fat32(state.efi_partition)
    partitioner.set_efi_type(state.efi_partition)

    info(f"Formatting root ({state.filesystem})...")
    partitioner.format_filesystem(root_dev, state.filesystem)

    if state.home_partition:
        info(f"Formatting home ({state.filesystem})...")
        partitioner.format_filesystem(state.home_partition, state.filesystem, label="home")
    if state.swap_partition:
        info("Setting up swap...")
        partitioner.setup_swap(state.swap_partition)

    info("Mounting...")
    if state.filesystem == "btrfs":
        partitioner.setup_btrfs_subvolumes(root_dev, bool(state.home_partition))
    else:
        partitioner.mount_simple(root_dev)
    partitioner.mount_partition(state.efi_partition, "/mnt/boot")
    if state.home_partition:
        partitioner.mount_partition(state.home_partition, "/mnt/home")
    success("Mounted.")

    if state.install_type == "fresh":
        info("Generating NixOS configuration...")
        config_generator.generate_config(state)

    info("Generating hardware configuration...")
    nixos.generate_config()
    nixos.copy_repo(state.selected_host)
    age_key = nixos.generate_ssh_keys()
    if age_key:
        warn(f"sops age key: {age_key}")

    info(f"Installing Bingux ({state.selected_host})...")
    print()
    ok = nixos.install(state.selected_host, log_callback=lambda l: sys.stdout.write(l))
    if not ok:
        fail("Installation failed.")
        return 1

    if state.username and state.password:
        info(f"Setting password for {state.username}...")
        nixos.set_password(state.username, state.password)

    print()
    print(f"  {GREEN}{BOLD}Installation complete!{RESET}")
    print()
    if confirm("Reboot now?"):
        subprocess.run(["reboot"])

    return 0
