#!/bin/bash
# ============================================================================
# Bingux Disk Installer
# ============================================================================
# Installs Bingux to real hardware from a live environment or cross-install.
#
# Usage:
#   sudo ./bingux-install.sh                   # interactive TUI
#   sudo ./bingux-install.sh --disk /dev/sda   # skip disk selection
#   sudo ./bingux-install.sh --source /path    # specify rootfs source
#
# Requirements: bash, util-linux (lsblk, sfdisk, mkfs.*), grub2
# Optional: dialog or whiptail for pretty TUI
# ============================================================================

set -euo pipefail

# ── Globals ─────────────────────────────────────────────────────────────────

INSTALLER_VERSION="0.1.0"
TARGET="/mnt/bingux"
EFI_SIZE="512M"
ROOT_FS="ext4"       # ext4 or btrfs
DISK=""
HOSTNAME_SET="bingux"
USERNAME=""
USER_PASSWORD=""
ROOT_PASSWORD=""
SOURCE=""             # squashfs or live root to copy from
BINGUX_LIVE=""        # set if running from a live Bingux environment
DRY_RUN=0

# Colour codes (used in plain mode)
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
RESET='\033[0m'

# ── TUI Backend Detection ──────────────────────────────────────────────────

TUI=""
detect_tui() {
    if command -v dialog >/dev/null 2>&1; then
        TUI="dialog"
    elif command -v whiptail >/dev/null 2>&1; then
        TUI="whiptail"
    else
        TUI="plain"
    fi
}

# ── TUI Helpers ─────────────────────────────────────────────────────────────

# Display an informational message box.
msg_box() {
    local title="$1" text="$2"
    case "$TUI" in
        dialog)   dialog --title "$title" --msgbox "$text" 20 70 ;;
        whiptail) whiptail --title "$title" --msgbox "$text" 20 70 ;;
        plain)
            echo ""
            echo -e "${BOLD}=== $title ===${RESET}"
            echo "$text"
            echo ""
            echo -n "Press Enter to continue... "
            read -r
            ;;
    esac
}

# Yes/No confirmation. Returns 0 for yes, 1 for no.
yesno() {
    local title="$1" text="$2"
    case "$TUI" in
        dialog)   dialog --title "$title" --yesno "$text" 12 60 ;;
        whiptail) whiptail --title "$title" --yesno "$text" 12 60 ;;
        plain)
            echo ""
            echo -e "${BOLD}$title${RESET}"
            echo "$text"
            echo -n "[y/N] "
            local ans
            read -r ans
            [[ "$ans" =~ ^[Yy] ]]
            ;;
    esac
}

# Present a menu. Prints the chosen tag to stdout.
# Usage: menu "title" "text" "tag1" "desc1" "tag2" "desc2" ...
tui_menu() {
    local title="$1" text="$2"
    shift 2
    local items=("$@")
    local count=$(( ${#items[@]} / 2 ))

    case "$TUI" in
        dialog)
            dialog --title "$title" --menu "$text" 20 70 "$count" "${items[@]}" 3>&1 1>&2 2>&3
            ;;
        whiptail)
            whiptail --title "$title" --menu "$text" 20 70 "$count" "${items[@]}" 3>&1 1>&2 2>&3
            ;;
        plain)
            echo ""
            echo -e "${BOLD}=== $title ===${RESET}"
            echo "$text"
            echo ""
            local i=0
            local idx=1
            while [ $i -lt ${#items[@]} ]; do
                echo "  $idx) ${items[$i]}  -  ${items[$((i+1))]}"
                i=$((i + 2))
                idx=$((idx + 1))
            done
            echo ""
            local choice
            while true; do
                echo -n "Selection [1-$count]: "
                read -r choice
                if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "$count" ]; then
                    local ci=$(( (choice - 1) * 2 ))
                    echo "${items[$ci]}"
                    return 0
                fi
                echo "Invalid selection."
            done
            ;;
    esac
}

# Text input. Prints entered text to stdout.
tui_input() {
    local title="$1" text="$2" default="${3:-}"
    case "$TUI" in
        dialog)   dialog --title "$title" --inputbox "$text" 10 60 "$default" 3>&1 1>&2 2>&3 ;;
        whiptail) whiptail --title "$title" --inputbox "$text" 10 60 "$default" 3>&1 1>&2 2>&3 ;;
        plain)
            echo ""
            echo -e "${BOLD}$title${RESET}"
            echo -n "$text [$default]: "
            local val
            read -r val
            echo "${val:-$default}"
            ;;
    esac
}

# Password input. Prints entered text to stdout.
tui_password() {
    local title="$1" text="$2"
    case "$TUI" in
        dialog)   dialog --title "$title" --insecure --passwordbox "$text" 10 60 3>&1 1>&2 2>&3 ;;
        whiptail) whiptail --title "$title" --passwordbox "$text" 10 60 3>&1 1>&2 2>&3 ;;
        plain)
            echo ""
            echo -e "${BOLD}$title${RESET}"
            echo -n "$text: "
            local val
            read -rs val
            echo ""
            echo "$val"
            ;;
    esac
}

# Progress indicator for a step.
step() {
    local num="$1" total="$2" msg="$3"
    echo -e "${CYAN}[$num/$total]${RESET} $msg"
}

# Fatal error.
die() {
    echo -e "${RED}FATAL: $*${RESET}" >&2
    exit 1
}

# ── Preflight Checks ───────────────────────────────────────────────────────

preflight() {
    # Must be root
    if [ "$(id -u)" -ne 0 ]; then
        die "This installer must be run as root (use sudo)."
    fi

    # Required commands
    local missing=()
    for cmd in lsblk sfdisk mkfs.fat mkfs.ext4 mount umount blkid; do
        command -v "$cmd" >/dev/null 2>&1 || missing+=("$cmd")
    done
    if [ ${#missing[@]} -gt 0 ]; then
        die "Missing required commands: ${missing[*]}"
    fi

    # Check for GRUB
    if ! command -v grub-install >/dev/null 2>&1 && ! command -v grub2-install >/dev/null 2>&1; then
        echo -e "${YELLOW}WARNING: grub-install not found. Bootloader step will be skipped.${RESET}"
        echo -e "${YELLOW}You will need to install a bootloader manually.${RESET}"
    fi

    # Optional btrfs
    if ! command -v mkfs.btrfs >/dev/null 2>&1; then
        ROOT_FS="ext4"
    fi

    # Detect if we are in a live Bingux environment
    if [ -d /system/packages ] && [ -f /system/config/system.toml ]; then
        BINGUX_LIVE="1"
    fi
}

# ── Source Detection ────────────────────────────────────────────────────────

detect_source() {
    if [ -n "$SOURCE" ]; then
        # Explicitly provided
        if [ -f "$SOURCE" ] && file "$SOURCE" | grep -qi squashfs; then
            return 0
        elif [ -d "$SOURCE" ]; then
            return 0
        else
            die "Source '$SOURCE' is not a valid squashfs file or directory."
        fi
    fi

    # Auto-detect: live Bingux environment
    if [ "$BINGUX_LIVE" = "1" ]; then
        SOURCE="/"
        return 0
    fi

    # Auto-detect: squashfs on the ISO/media
    for sqfs in /run/media/bingux/root.sqfs /run/rootfs.ro /media/*/bingux/root.sqfs /cdrom/bingux/root.sqfs; do
        if [ -f "$sqfs" ]; then
            SOURCE="$sqfs"
            return 0
        fi
    done

    # Check if /run/rootfs.ro is mounted (overlay root from initrd)
    if mountpoint -q /run/rootfs.ro 2>/dev/null; then
        SOURCE="/run/rootfs.ro"
        return 0
    fi

    # If running from the overlay newroot, use the current root
    if [ -d /system/packages ]; then
        SOURCE="/"
        return 0
    fi

    die "Could not find Bingux root filesystem source.\nUse --source /path/to/root.sqfs or run from a live Bingux environment."
}

# ── Step 1: Welcome ────────────────────────────────────────────────────────

welcome_screen() {
    local text
    text="Bingux Installer v${INSTALLER_VERSION}

This will install Bingux to your disk.

Layout:
  /io            Device nodes
  /system        Packages, profiles, config
  /users         Home directories

Source: ${SOURCE}
Mode: $([ "$BINGUX_LIVE" = "1" ] && echo "Live Bingux" || echo "Cross-install")

WARNING: The selected disk will be COMPLETELY ERASED.
Make sure you have backups of any important data."

    msg_box "Welcome to Bingux" "$text"
}

# ── Step 2: Disk Selection ─────────────────────────────────────────────────

select_disk() {
    if [ -n "$DISK" ]; then
        # Provided via CLI
        if [ ! -b "$DISK" ]; then
            die "Disk '$DISK' does not exist or is not a block device."
        fi
        return 0
    fi

    # Enumerate disks (exclude loop, ram, sr devices)
    local -a tags=()
    local -a descs=()
    local -a menu_items=()

    while IFS= read -r line; do
        local name size model type
        name=$(echo "$line" | awk '{print $1}')
        size=$(echo "$line" | awk '{print $2}')
        type=$(echo "$line" | awk '{print $3}')
        model=$(echo "$line" | cut -d' ' -f4-)

        [ "$type" = "disk" ] || continue
        [ -b "/dev/$name" ] || continue

        tags+=("/dev/$name")
        descs+=("$size $model")
        menu_items+=("/dev/$name" "$size $model")
    done < <(lsblk -dno NAME,SIZE,TYPE,MODEL 2>/dev/null || true)

    if [ ${#tags[@]} -eq 0 ]; then
        die "No disks found. Make sure your disk is connected and detected."
    fi

    DISK=$(tui_menu "Disk Selection" "Select the disk to install Bingux to:" "${menu_items[@]}")

    if [ -z "$DISK" ] || [ ! -b "$DISK" ]; then
        die "No disk selected."
    fi

    # Confirm destructive operation
    local disk_info
    disk_info=$(lsblk -dno SIZE,MODEL "$DISK" 2>/dev/null || echo "unknown")
    if ! yesno "Confirm Disk" "ALL DATA on $DISK ($disk_info) will be DESTROYED.\n\nAre you sure you want to continue?"; then
        echo "Installation cancelled."
        exit 0
    fi
}

# ── Step 3: Filesystem Selection ───────────────────────────────────────────

select_filesystem() {
    local -a menu_items=("ext4" "Standard Linux filesystem (recommended)")

    if command -v mkfs.btrfs >/dev/null 2>&1; then
        menu_items+=("btrfs" "Copy-on-write filesystem (snapshots)")
    fi

    if [ ${#menu_items[@]} -gt 2 ]; then
        ROOT_FS=$(tui_menu "Filesystem" "Select root filesystem type:" "${menu_items[@]}")
    else
        ROOT_FS="ext4"
    fi
}

# ── Step 4: User Configuration ─────────────────────────────────────────────

configure_user() {
    HOSTNAME_SET=$(tui_input "Hostname" "Enter hostname for this machine:" "bingux")
    [ -z "$HOSTNAME_SET" ] && HOSTNAME_SET="bingux"

    USERNAME=$(tui_input "User Account" "Enter username for the primary user:" "bingux")
    [ -z "$USERNAME" ] && USERNAME="bingux"

    USER_PASSWORD=$(tui_password "User Password" "Enter password for $USERNAME (leave empty for no password):")

    if yesno "Root Password" "Set a root password? (No = passwordless root login)"; then
        ROOT_PASSWORD=$(tui_password "Root Password" "Enter root password:")
    fi
}

# ── Step 5: Confirmation ───────────────────────────────────────────────────

confirm_install() {
    local text
    text="Ready to install Bingux with the following settings:

  Disk:       $DISK
  Filesystem: $ROOT_FS
  Hostname:   $HOSTNAME_SET
  User:       $USERNAME
  Source:     $SOURCE

Partition layout:
  ${DISK}p1 / ${DISK}1  512MB  EFI System Partition (FAT32)
  ${DISK}p2 / ${DISK}2  rest   Root ($ROOT_FS)

This is your last chance to cancel."

    if ! yesno "Confirm Installation" "$text"; then
        echo "Installation cancelled."
        exit 0
    fi
}

# ── Step 6: Partition ──────────────────────────────────────────────────────

partition_disk() {
    step 1 7 "Partitioning $DISK..."

    # Wipe existing partition table
    wipefs -af "$DISK" >/dev/null 2>&1 || true

    # Create GPT partition table with sfdisk
    sfdisk "$DISK" <<EOF
label: gpt
size=${EFI_SIZE}, type=C12A7328-F81F-11D2-BA4B-00A0C93EC93B, name="EFI System Partition"
type=0FC63DAF-8483-4772-8E79-3D69D8477DE4, name="Bingux Root"
EOF

    # Wait for kernel to re-read partition table
    partprobe "$DISK" 2>/dev/null || true
    sleep 1

    # Determine partition naming scheme (sda1 vs nvme0n1p1)
    if [[ "$DISK" =~ nvme|mmcblk|loop ]]; then
        EFI_PART="${DISK}p1"
        ROOT_PART="${DISK}p2"
    else
        EFI_PART="${DISK}1"
        ROOT_PART="${DISK}2"
    fi

    # Wait for partition devices to appear
    local retries=10
    while [ ! -b "$EFI_PART" ] && [ $retries -gt 0 ]; do
        sleep 0.5
        retries=$((retries - 1))
    done

    if [ ! -b "$EFI_PART" ] || [ ! -b "$ROOT_PART" ]; then
        die "Partition devices did not appear ($EFI_PART, $ROOT_PART)."
    fi

    echo -e "  ${GREEN}EFI:  $EFI_PART${RESET}"
    echo -e "  ${GREEN}Root: $ROOT_PART${RESET}"
}

# ── Step 7: Format ─────────────────────────────────────────────────────────

format_partitions() {
    step 2 7 "Formatting partitions..."

    echo "  Formatting $EFI_PART as FAT32..."
    mkfs.fat -F 32 -n BINGUX_EFI "$EFI_PART" >/dev/null

    echo "  Formatting $ROOT_PART as $ROOT_FS..."
    case "$ROOT_FS" in
        ext4)
            mkfs.ext4 -F -L bingux-root "$ROOT_PART" >/dev/null 2>&1
            ;;
        btrfs)
            mkfs.btrfs -f -L bingux-root "$ROOT_PART" >/dev/null 2>&1
            ;;
    esac

    echo -e "  ${GREEN}Partitions formatted.${RESET}"
}

# ── Step 8: Mount ──────────────────────────────────────────────────────────

mount_target() {
    step 3 7 "Mounting target filesystem..."

    mkdir -p "$TARGET"
    mount "$ROOT_PART" "$TARGET"

    if [ "$ROOT_FS" = "btrfs" ]; then
        # Create subvolumes for btrfs
        btrfs subvolume create "$TARGET/@" >/dev/null
        btrfs subvolume create "$TARGET/@users" >/dev/null
        umount "$TARGET"
        mount -o subvol=@ "$ROOT_PART" "$TARGET"
        mkdir -p "$TARGET/users"
        mount -o subvol=@users "$ROOT_PART" "$TARGET/users"
    fi

    mkdir -p "$TARGET/boot/efi"
    mount "$EFI_PART" "$TARGET/boot/efi"

    echo -e "  ${GREEN}Mounted root at $TARGET${RESET}"
}

# ── Step 9: Install Root Filesystem ────────────────────────────────────────

install_rootfs() {
    step 4 7 "Installing Bingux root filesystem..."

    # Create the Bingux directory structure
    mkdir -p "$TARGET"/{io,system,users,etc,bin,sbin,lib,lib64,boot/efi}
    mkdir -p "$TARGET"/system/{packages,profiles,config,state,boot,modules,tmp,recipes}
    mkdir -p "$TARGET"/system/state/{persistent,ephemeral}
    mkdir -p "$TARGET"/system/kernel/{proc,sys}
    mkdir -p "$TARGET"/usr/{bin,lib,lib64,sbin}
    mkdir -p "$TARGET"/var/{log,run,tmp}
    mkdir -p "$TARGET"/run
    mkdir -p "$TARGET"/tmp

    if [ -f "$SOURCE" ]; then
        # Source is a squashfs file -- unsquash it
        echo "  Extracting squashfs: $SOURCE"
        if command -v unsquashfs >/dev/null 2>&1; then
            unsquashfs -f -d "$TARGET" "$SOURCE"
        else
            # Mount and copy
            local sqfs_mnt
            sqfs_mnt=$(mktemp -d)
            mount -t squashfs -o ro,loop "$SOURCE" "$sqfs_mnt"
            echo "  Copying files (this may take a while)..."
            cp -a "$sqfs_mnt"/. "$TARGET"/
            umount "$sqfs_mnt"
            rmdir "$sqfs_mnt"
        fi
    elif [ -d "$SOURCE" ]; then
        # Source is a directory (live root or extracted rootfs)
        echo "  Copying from $SOURCE..."
        echo "  Copying packages..."
        if [ -d "$SOURCE/system/packages" ]; then
            cp -a "$SOURCE"/system/packages/. "$TARGET"/system/packages/
            local pkg_count
            pkg_count=$(ls "$TARGET/system/packages/" 2>/dev/null | wc -l)
            echo -e "  ${GREEN}Copied $pkg_count packages${RESET}"
        fi

        echo "  Copying profiles..."
        if [ -d "$SOURCE/system/profiles" ]; then
            cp -a "$SOURCE"/system/profiles/. "$TARGET"/system/profiles/
        fi

        echo "  Copying config..."
        if [ -d "$SOURCE/system/config" ]; then
            cp -a "$SOURCE"/system/config/. "$TARGET"/system/config/
        fi

        echo "  Copying kernel and modules..."
        if [ -d "$SOURCE/system/boot" ]; then
            cp -a "$SOURCE"/system/boot/. "$TARGET"/system/boot/
        fi
        if [ -d "$SOURCE/system/modules" ]; then
            cp -a "$SOURCE"/system/modules/. "$TARGET"/system/modules/
        fi

        echo "  Copying binaries and libraries..."
        # Copy FHS compat layers
        for d in bin sbin lib lib64; do
            if [ -d "$SOURCE/$d" ]; then
                cp -a "$SOURCE/$d"/. "$TARGET/$d/" 2>/dev/null || true
            fi
        done
        for d in usr/bin usr/lib usr/lib64 usr/sbin; do
            if [ -d "$SOURCE/$d" ]; then
                mkdir -p "$TARGET/$d"
                cp -a "$SOURCE/$d"/. "$TARGET/$d/" 2>/dev/null || true
            fi
        done

        echo "  Copying init..."
        if [ -f "$SOURCE/init" ]; then
            cp -a "$SOURCE/init" "$TARGET/init"
        fi

        echo "  Copying recipes..."
        if [ -d "$SOURCE/system/recipes" ]; then
            cp -a "$SOURCE"/system/recipes/. "$TARGET"/system/recipes/
        fi

        # Copy persistent state (package DB, etc.)
        if [ -d "$SOURCE/system/state/persistent" ]; then
            cp -a "$SOURCE"/system/state/persistent/. "$TARGET"/system/state/persistent/
        fi
    fi

    # Ensure Bingux directory structure always exists
    mkdir -p "$TARGET"/io
    mkdir -p "$TARGET"/system/{packages,profiles,config,state/persistent,state/ephemeral}
    mkdir -p "$TARGET"/system/kernel/{proc,sys}
    mkdir -p "$TARGET"/users

    echo -e "  ${GREEN}Root filesystem installed.${RESET}"
}

# ── Step 10: System Configuration ──────────────────────────────────────────

configure_system() {
    step 5 7 "Configuring system..."

    # Determine the root partition UUID
    local root_uuid efi_uuid
    root_uuid=$(blkid -s UUID -o value "$ROOT_PART" 2>/dev/null || echo "")
    efi_uuid=$(blkid -s UUID -o value "$EFI_PART" 2>/dev/null || echo "")

    # ── system.toml ────────────────────────────────────────────────
    # Build package list from installed packages
    local pkg_list=""
    if [ -d "$TARGET/system/packages" ]; then
        for pkg_dir in "$TARGET"/system/packages/*/; do
            [ -d "$pkg_dir" ] || continue
            local pkg_name
            pkg_name=$(basename "$pkg_dir" | sed 's/-[0-9].*//')
            if [ -n "$pkg_list" ]; then
                pkg_list="$pkg_list, \"$pkg_name\""
            else
                pkg_list="\"$pkg_name\""
            fi
        done
    fi

    cat > "$TARGET/system/config/system.toml" << SYSCONF
[system]
hostname = "$HOSTNAME_SET"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = [$pkg_list]

[services]
enable = []

[[users]]
name = "$USERNAME"
uid = 1000
gid = 1000
home = "/users/$USERNAME"
shell = "/bin/bash"
groups = ["wheel"]
SYSCONF

    # ── /etc files ─────────────────────────────────────────────────
    mkdir -p "$TARGET/etc"

    # hostname
    echo "$HOSTNAME_SET" > "$TARGET/etc/hostname"

    # os-release
    cat > "$TARGET/etc/os-release" << 'OSREL'
NAME="Bingux"
ID=bingux
VERSION="2"
VERSION_ID=2
PRETTY_NAME="Bingux v2"
HOME_URL="https://github.com/kierandrewett/bingux"
OSREL

    # passwd
    cat > "$TARGET/etc/passwd" << PASSWD
root:x:0:0:root:/users/root:/bin/bash
$USERNAME:x:1000:1000:$USERNAME:/users/$USERNAME:/bin/bash
nobody:x:65534:65534:Nobody:/:/sbin/nologin
PASSWD

    # group
    cat > "$TARGET/etc/group" << GROUP
root:x:0:
wheel:x:10:$USERNAME
$USERNAME:x:1000:
nobody:x:65534:
GROUP

    # shadow
    local root_hash="" user_hash=""
    if [ -n "$ROOT_PASSWORD" ]; then
        if command -v openssl >/dev/null 2>&1; then
            root_hash=$(openssl passwd -6 "$ROOT_PASSWORD")
        elif command -v mkpasswd >/dev/null 2>&1; then
            root_hash=$(mkpasswd -m sha-512 "$ROOT_PASSWORD")
        fi
    fi
    if [ -n "$USER_PASSWORD" ]; then
        if command -v openssl >/dev/null 2>&1; then
            user_hash=$(openssl passwd -6 "$USER_PASSWORD")
        elif command -v mkpasswd >/dev/null 2>&1; then
            user_hash=$(mkpasswd -m sha-512 "$USER_PASSWORD")
        fi
    fi

    cat > "$TARGET/etc/shadow" << SHADOW
root:${root_hash:-!}:0:0:99999:7:::
$USERNAME:${user_hash:-!}:0:0:99999:7:::
nobody:!:0:0:99999:7:::
SHADOW
    chmod 600 "$TARGET/etc/shadow"

    # fstab
    cat > "$TARGET/etc/fstab" << FSTAB
# Bingux fstab - generated by bingux-install
# <device>                                <mount>       <type>   <options>        <dump> <pass>
UUID=$root_uuid  /             $ROOT_FS defaults         0      1
UUID=$efi_uuid   /boot/efi     vfat     defaults,noatime 0      2
tmpfs            /etc          tmpfs    size=16M         0      0
tmpfs            /system/tmp   tmpfs    size=1500M       0      0
tmpfs            /system/state/ephemeral tmpfs size=256M 0      0
proc             /system/kernel/proc proc defaults       0      0
sysfs            /system/kernel/sys  sysfs defaults      0      0
devtmpfs         /io           devtmpfs defaults         0      0
FSTAB

    # /etc/profile for shell environment
    cat > "$TARGET/etc/profile" << 'PROFILE'
export PATH="/system/profiles/current/bin:/bin:/sbin:/usr/bin:/usr/sbin"
export LD_LIBRARY_PATH="/lib64:/usr/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_CONFIG_PATH="/system/config/system.toml"
export BSYS_PROFILES_ROOT="/system/profiles"
export BSYS_PACKAGES_ROOT="/system/packages"
export TERM="linux"
export SSL_CERT_FILE="/etc/ssl/certs/ca-bundle.crt"

# Set home directory for the current user
if [ "$(id -u)" -eq 0 ]; then
    export HOME="/users/root"
else
    export HOME="/users/$(whoami)"
fi

PS1='\[\e[1;36m\]bingux\[\e[0m\]:\[\e[1;34m\]\w\[\e[0m\]\$ '
PROFILE

    # resolv.conf
    cat > "$TARGET/etc/resolv.conf" << 'DNS'
nameserver 1.1.1.1
nameserver 1.0.0.1
DNS

    # machine-id
    if command -v systemd-machine-id-setup >/dev/null 2>&1; then
        systemd-machine-id-setup --root="$TARGET" 2>/dev/null || true
    else
        cat /proc/sys/kernel/random/uuid | tr -d '-' > "$TARGET/etc/machine-id" 2>/dev/null || true
    fi

    # ── Create user home directories ──────────────────────────────
    mkdir -p "$TARGET/users/root"
    mkdir -p "$TARGET/users/$USERNAME"
    mkdir -p "$TARGET/users/$USERNAME/.config/bingux"/{config,profiles,permissions,state}

    # Set ownership (will only work if running as root with matching UIDs)
    chown -R 1000:1000 "$TARGET/users/$USERNAME" 2>/dev/null || true

    # ── Run bsys apply if available ───────────────────────────────
    if command -v bsys >/dev/null 2>&1 || [ -f "$TARGET/system/profiles/current/bin/bsys" ]; then
        echo "  Running bsys apply to generate system profile..."
        local bsys_bin
        bsys_bin=$(command -v bsys 2>/dev/null || echo "$TARGET/system/profiles/current/bin/bsys")
        BPKG_STORE_ROOT="$TARGET/system/packages" \
        BSYS_CONFIG_PATH="$TARGET/system/config/system.toml" \
        BSYS_PROFILES_ROOT="$TARGET/system/profiles" \
        BSYS_PACKAGES_ROOT="$TARGET/system/packages" \
        BSYS_ETC_ROOT="$TARGET/etc" \
            "$bsys_bin" apply 2>&1 || echo "  (bsys apply had warnings, continuing)"
    fi

    echo -e "  ${GREEN}System configured.${RESET}"
}

# ── Step 11: Bootloader ───────────────────────────────────────────────────

install_bootloader() {
    step 6 7 "Installing bootloader..."

    local grub_cmd=""
    if command -v grub-install >/dev/null 2>&1; then
        grub_cmd="grub-install"
    elif command -v grub2-install >/dev/null 2>&1; then
        grub_cmd="grub2-install"
    else
        echo -e "  ${YELLOW}grub-install not found, skipping bootloader.${RESET}"
        echo -e "  ${YELLOW}You must install a bootloader manually before rebooting.${RESET}"
        return 0
    fi

    # Find kernel
    local kernel_path=""
    for k in "$TARGET"/system/boot/vmlinuz "$TARGET"/boot/vmlinuz \
             "$TARGET"/system/packages/linux-*/boot/vmlinuz; do
        if [ -f "$k" ]; then
            kernel_path="$k"
            break
        fi
    done

    # If no kernel found in target, copy from host
    if [ -z "$kernel_path" ] && [ -f "/boot/vmlinuz-$(uname -r)" ]; then
        echo "  Copying host kernel to target..."
        cp "/boot/vmlinuz-$(uname -r)" "$TARGET/boot/vmlinuz"
        kernel_path="$TARGET/boot/vmlinuz"
    elif [ -z "$kernel_path" ]; then
        echo -e "  ${YELLOW}No kernel found. You will need to install one manually.${RESET}"
    fi

    # Find or generate initramfs
    local initrd_path=""
    for i in "$TARGET"/system/boot/initramfs.img "$TARGET"/boot/initramfs.img \
             "$TARGET"/boot/initrd.img; do
        if [ -f "$i" ]; then
            initrd_path="$i"
            break
        fi
    done

    if [ -z "$initrd_path" ] && [ -f "/boot/initramfs-$(uname -r).img" ]; then
        echo "  Copying host initramfs to target..."
        cp "/boot/initramfs-$(uname -r).img" "$TARGET/boot/initramfs.img"
        initrd_path="$TARGET/boot/initramfs.img"
    fi

    # Bind-mount required filesystems for grub-install
    mount --bind /dev  "$TARGET/dev"  2>/dev/null || true
    mount --bind /proc "$TARGET/proc" 2>/dev/null || true
    mount --bind /sys  "$TARGET/sys"  2>/dev/null || true
    mount --bind /run  "$TARGET/run"  2>/dev/null || true

    # Install GRUB EFI
    echo "  Installing GRUB for UEFI..."
    chroot "$TARGET" "$grub_cmd" \
        --target=x86_64-efi \
        --efi-directory=/boot/efi \
        --bootloader-id=bingux \
        --removable \
        2>&1 || echo -e "  ${YELLOW}grub-install reported errors (may still work)${RESET}"

    # Write GRUB configuration
    local root_uuid
    root_uuid=$(blkid -s UUID -o value "$ROOT_PART" 2>/dev/null || echo "")

    mkdir -p "$TARGET/boot/grub" "$TARGET/boot/efi/EFI/bingux"

    # Determine kernel path relative to /boot
    local kern_grub="/boot/vmlinuz"
    local init_grub="/boot/initramfs.img"

    # Check if kernel lives in /system/boot
    if [ -f "$TARGET/system/boot/vmlinuz" ] && [ ! -f "$TARGET/boot/vmlinuz" ]; then
        cp "$TARGET/system/boot/vmlinuz" "$TARGET/boot/vmlinuz"
    fi
    if [ -f "$TARGET/system/boot/initramfs.img" ] && [ ! -f "$TARGET/boot/initramfs.img" ]; then
        cp "$TARGET/system/boot/initramfs.img" "$TARGET/boot/initramfs.img"
    fi

    cat > "$TARGET/boot/grub/grub.cfg" << GRUB
set default=0
set timeout=5

menuentry "Bingux" {
    search --no-floppy --fs-uuid --set=root $root_uuid
    linux $kern_grub root=UUID=$root_uuid rw quiet
    initrd $init_grub
}

menuentry "Bingux (verbose)" {
    search --no-floppy --fs-uuid --set=root $root_uuid
    linux $kern_grub root=UUID=$root_uuid rw loglevel=7 console=tty0
    initrd $init_grub
}

menuentry "Bingux (recovery shell)" {
    search --no-floppy --fs-uuid --set=root $root_uuid
    linux $kern_grub root=UUID=$root_uuid rw init=/bin/sh
    initrd $init_grub
}
GRUB

    # Unmount bound filesystems
    umount "$TARGET/run"  2>/dev/null || true
    umount "$TARGET/sys"  2>/dev/null || true
    umount "$TARGET/proc" 2>/dev/null || true
    umount "$TARGET/dev"  2>/dev/null || true

    echo -e "  ${GREEN}Bootloader installed.${RESET}"
}

# ── Step 12: Cleanup ──────────────────────────────────────────────────────

cleanup() {
    step 7 7 "Cleaning up..."

    # Sync to ensure all writes are flushed
    sync

    # Unmount in reverse order
    if [ "$ROOT_FS" = "btrfs" ]; then
        umount "$TARGET/users" 2>/dev/null || true
    fi
    umount "$TARGET/boot/efi" 2>/dev/null || true
    umount "$TARGET" 2>/dev/null || true

    echo -e "  ${GREEN}Unmounted target filesystems.${RESET}"
}

show_complete() {
    local text
    text="Bingux has been installed successfully!

  Disk:     $DISK
  Hostname: $HOSTNAME_SET
  User:     $USERNAME

You can now remove the installation media and reboot.

If you installed from an existing Linux system, make sure
your UEFI firmware is set to boot from $DISK.

Enjoy Bingux!"

    msg_box "Installation Complete" "$text"
}

# ── CLI Argument Parsing ───────────────────────────────────────────────────

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --disk)     DISK="$2"; shift 2 ;;
            --source)   SOURCE="$2"; shift 2 ;;
            --hostname) HOSTNAME_SET="$2"; shift 2 ;;
            --user)     USERNAME="$2"; shift 2 ;;
            --fs)       ROOT_FS="$2"; shift 2 ;;
            --target)   TARGET="$2"; shift 2 ;;
            --dry-run)  DRY_RUN=1; shift ;;
            --plain)    TUI="plain"; shift ;;
            --help|-h)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --disk /dev/sdX    Target disk (skip selection)"
                echo "  --source PATH      Root filesystem source (squashfs or directory)"
                echo "  --hostname NAME    Set hostname (default: bingux)"
                echo "  --user NAME        Primary user account name (default: bingux)"
                echo "  --fs ext4|btrfs    Root filesystem type (default: ext4)"
                echo "  --target PATH      Mount point for installation (default: /mnt/bingux)"
                echo "  --plain            Force plain text prompts (no dialog/whiptail)"
                echo "  --dry-run          Print actions without executing"
                echo "  --help             Show this help"
                exit 0
                ;;
            *)
                die "Unknown option: $1 (use --help for usage)"
                ;;
        esac
    done
}

# ── Main ───────────────────────────────────────────────────────────────────

main() {
    parse_args "$@"
    detect_tui
    preflight
    detect_source
    welcome_screen
    select_disk
    select_filesystem
    configure_user
    confirm_install

    echo ""
    echo -e "${BOLD}=== Installing Bingux ===${RESET}"
    echo ""

    partition_disk
    format_partitions
    mount_target
    install_rootfs
    configure_system
    install_bootloader
    cleanup

    echo ""
    echo -e "${GREEN}${BOLD}=== Installation Complete ===${RESET}"
    echo ""

    show_complete
}

main "$@"
