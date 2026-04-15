#!/bin/sh
# Bingux v2 — Production Init
# Handles: persistent disk, networking, profile system, systemd handoff
set -e

export PATH="/system/profiles/current/bin:/bin:/sbin:/usr/bin:/usr/sbin"
export LD_LIBRARY_PATH="/lib64:/lib64/systemd"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_WORK_DIR="/tmp/bsys-work"
export BSYS_CACHE_DIR="/tmp/bsys-cache"
export HOME="/root"
export TERM=linux

# Phase 1: Mount essential filesystems
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs -o size=1500M tmpfs /tmp
mount -t tmpfs tmpfs /run
mkdir -p /dev/pts /dev/shm
mount -t devpts devpts /dev/pts 2>/dev/null || true
mount -t tmpfs tmpfs /dev/shm 2>/dev/null || true

echo "bingux" > /proc/sys/kernel/hostname

echo ""
echo "  ____  _                        "
echo " | __ )(_)_ __   __ _ _   ___  __"
echo " |  _ \| | '_ \ / _\` | | | \ \/ /"
echo " | |_) | | | | | (_| | |_| |>  < "
echo " |____/|_|_| |_|\__, |\__,_/_/\_\\"
echo "                |___/             "
echo ""
echo "  Bingux v2 — Self-Hosting Linux"
echo ""

# Phase 2: Persistent disk
DISK=""
for dev in /dev/vda /dev/sda; do
    [ -b "$dev" ] && DISK="$dev" && break
done

if [ -n "$DISK" ]; then
    echo "[init] Persistent disk: $DISK"

    mkdir -p /mnt/persistent
    # Try to mount first
    if mount "$DISK" /mnt/persistent 2>/dev/null; then
        echo "[init] Persistent storage mounted (existing)"
    else
        # Not formatted yet — format and mount
        echo "[init] Formatting $DISK as ext2..."
        mkfs.ext2 -q "$DISK" 2>/dev/null || mke2fs -q "$DISK" 2>/dev/null || true
        mount "$DISK" /mnt/persistent 2>/dev/null || true
    fi

    if mountpoint -q /mnt/persistent 2>/dev/null; then

        # Create dirs on persistent disk
        mkdir -p /mnt/persistent/system/packages
        mkdir -p /mnt/persistent/system/profiles
        mkdir -p /mnt/persistent/system/config
        mkdir -p /mnt/persistent/cache
        mkdir -p /mnt/persistent/builds

        # First boot: sync packages from initramfs
        if [ ! -f /mnt/persistent/.initialized ]; then
            echo "[init] First boot — syncing packages to persistent disk..."
            cp -a /system/packages/* /mnt/persistent/system/packages/ 2>/dev/null || true
            cp -a /system/config/* /mnt/persistent/system/config/ 2>/dev/null || true
            cp -a /system/profiles/* /mnt/persistent/system/profiles/ 2>/dev/null || true
            date > /mnt/persistent/.initialized
            echo "[init] Sync complete"
        fi

        # Bind-mount persistent store over initramfs
        mount --bind /mnt/persistent/system/packages /system/packages 2>/dev/null || true
        mount --bind /mnt/persistent/system/profiles /system/profiles 2>/dev/null || true

        export BSYS_WORK_DIR="/mnt/persistent/builds"
        export BSYS_CACHE_DIR="/mnt/persistent/cache"
    else
        echo "[init] WARNING: Could not mount persistent disk"
    fi
else
    echo "[init] No persistent disk (add -drive file=disk.qcow2,if=virtio)"
fi

# Phase 3: Networking
echo "[init] Setting up networking..."
ip link set lo up 2>/dev/null || true
for nic in eth0 ens3 enp0s3; do
    if ip link show "$nic" >/dev/null 2>&1; then
        ip link set "$nic" up 2>/dev/null || true
        if which udhcpc >/dev/null 2>&1; then
            udhcpc -i "$nic" -q -n 2>/dev/null && echo "[init] Network: $nic via DHCP" && break
        fi
    fi
done

# Phase 4: System info
PKGS=$(ls /system/packages/ 2>/dev/null | wc -l)
BINS=$(ls /system/profiles/current/bin/ 2>/dev/null | wc -l)
echo ""
echo "  Packages: $PKGS"
echo "  Binaries: $BINS"
echo "  Kernel:   $(uname -r)"
if [ -n "$DISK" ] && mountpoint -q /mnt/persistent 2>/dev/null; then
    DISK_USED=$(df -h /mnt/persistent 2>/dev/null | tail -1 | awk '{print $3"/"$2}')
    echo "  Disk:     $DISK_USED"
fi
echo ""

# Phase 5: Boot mode
SYSTEMD_BIN="/system/packages/systemd-src-256.11-x86_64-linux/lib/systemd/systemd"

if grep -q "init.systemd" /proc/cmdline 2>/dev/null && [ -f "$SYSTEMD_BIN" ]; then
    echo "[init] Handing off to systemd..."
    mkdir -p /sys/fs/cgroup
    mount -t cgroup2 cgroup2 /sys/fs/cgroup 2>/dev/null || true
    mkdir -p /run/systemd /var/log/journal /var/run /var/tmp /etc/systemd /run/dbus

    # Ensure serial console device exists
    [ -c /dev/ttyS0 ] || mknod /dev/ttyS0 c 4 64 2>/dev/null || true
    [ -c /dev/console ] || mknod /dev/console c 5 1 2>/dev/null || true
    [ -c /dev/null ] || mknod /dev/null c 1 3 2>/dev/null || true

    # Symlink systemd tools into /usr/bin so unit files can find them
    SYSD="/system/packages/systemd-src-256.11-x86_64-linux"
    mkdir -p /usr/bin /usr/sbin
    for b in systemd-tmpfiles udevadm journalctl systemctl busctl systemd-escape \
             systemd-run systemd-cat systemd-ask-password systemd-machine-id-setup \
             systemd-notify loginctl; do
        [ -f "$SYSD/bin/$b" ] && ln -sf "$SYSD/bin/$b" "/usr/bin/$b" 2>/dev/null
    done
    # Symlink daemons into /usr/lib/systemd/
    mkdir -p /usr/lib/systemd
    for d in systemd-journald systemd-udevd systemd-executor systemd-shutdown \
             systemd-remount-fs systemd-makefs systemd-growfs systemd-fsck; do
        [ -f "$SYSD/lib/systemd/$d" ] && ln -sf "$SYSD/lib/systemd/$d" "/usr/lib/systemd/$d" 2>/dev/null
    done

    exec "$SYSTEMD_BIN" --system --unit=multi-user.target
else
    echo "[init] Interactive shell (add 'init.systemd' to boot for systemd)"
    echo "  bingux-gcc  — compile C programs"
    echo "  bsys build  — build packages from BPKGBUILDs"
    echo "  bpkg list   — list installed packages"
    echo ""
    exec /bin/sh -l
fi
