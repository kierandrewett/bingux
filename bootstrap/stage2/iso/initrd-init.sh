#!/bin/sh
# Bingux v2 -- Minimal initrd init
# Purpose: mount the ISO, find root.sqfs, mount it, switch_root into it.
# This runs from a tiny initrd (< 10MB) containing just busybox + this script.
set -e

export PATH="/bin:/sbin"

# ── Mount kernel pseudo-filesystems ──────────────────────────────
mount -t proc     proc     /proc
mount -t sysfs    sysfs    /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs    tmpfs    /run

# Create device nodes if devtmpfs failed
[ -e /dev/null ]    || mknod /dev/null    c 1 3
[ -e /dev/console ] || mknod /dev/console c 5 1
[ -e /dev/sr0 ]     || mknod /dev/sr0     b 11 0
[ -e /dev/loop0 ]   || mknod /dev/loop0   b 7 0

echo "[initrd] Bingux v2 early init"

# ── Load necessary kernel modules ────────────────────────────────
# squashfs for the root filesystem, isofs/sr_mod for CD-ROM, loop for loopback
for mod in squashfs loop isofs sr_mod cdrom overlay; do
    modprobe "$mod" 2>/dev/null || true
done

# ── Find and mount the ISO/boot media ───────────────────────────
# We look for a partition/device containing /bingux/root.sqfs
MEDIA_MNT="/run/media"
SQFS_PATH=""
mkdir -p "$MEDIA_MNT"

# Wait a moment for devices to settle
sleep 1

find_squashfs() {
    local dev="$1"
    mount -o ro "$dev" "$MEDIA_MNT" 2>/dev/null || return 1
    if [ -f "$MEDIA_MNT/bingux/root.sqfs" ]; then
        SQFS_PATH="$MEDIA_MNT/bingux/root.sqfs"
        echo "[initrd] Found root.sqfs on $dev"
        return 0
    fi
    umount "$MEDIA_MNT" 2>/dev/null
    return 1
}

# Try CD-ROM devices first (ISO boot)
for dev in /dev/sr0 /dev/sr1 /dev/cdrom; do
    [ -b "$dev" ] && find_squashfs "$dev" && break
done

# Try disk partitions if not found on CD
if [ -z "$SQFS_PATH" ]; then
    for dev in /dev/vda /dev/vda1 /dev/sda /dev/sda1 /dev/nvme0n1 /dev/nvme0n1p1; do
        [ -b "$dev" ] && find_squashfs "$dev" && break
    done
fi

# Try scanning /sys/block for any block device we missed
if [ -z "$SQFS_PATH" ]; then
    for blockdev in /sys/block/*/; do
        devname="/dev/$(basename "$blockdev")"
        [ -b "$devname" ] && find_squashfs "$devname" && break
        # Try partitions
        for part in "$blockdev"/*/; do
            partname="/dev/$(basename "$part")"
            [ -b "$partname" ] && find_squashfs "$partname" && break 2
        done
    done
fi

if [ -z "$SQFS_PATH" ]; then
    echo "[initrd] FATAL: could not find /bingux/root.sqfs on any device"
    echo "[initrd] Available block devices:"
    ls -la /dev/sd* /dev/vd* /dev/sr* /dev/nvme* 2>/dev/null || echo "  (none)"
    echo "[initrd] Dropping to emergency shell"
    exec /bin/sh
fi

# ── Mount squashfs as root ───────────────────────────────────────
SQFS_MNT="/run/rootfs.ro"
OVERLAY_UPPER="/run/rootfs.rw/upper"
OVERLAY_WORK="/run/rootfs.rw/work"
NEWROOT="/run/newroot"

mkdir -p "$SQFS_MNT" "$OVERLAY_UPPER" "$OVERLAY_WORK" "$NEWROOT"

echo "[initrd] Mounting squashfs..."
mount -t squashfs -o ro,loop "$SQFS_PATH" "$SQFS_MNT"

# Use overlayfs to provide a writable root on top of the read-only squashfs
echo "[initrd] Setting up overlay (rw layer on tmpfs)..."
mount -t tmpfs -o size=512M tmpfs /run/rootfs.rw
mkdir -p "$OVERLAY_UPPER" "$OVERLAY_WORK"
mount -t overlay overlay -o "lowerdir=$SQFS_MNT,upperdir=$OVERLAY_UPPER,workdir=$OVERLAY_WORK" "$NEWROOT"

# ── Prepare for switch_root ─────────────────────────────────────
# Move mounted filesystems into the new root so they persist across switch_root
mkdir -p "$NEWROOT/run/media" "$NEWROOT/run/rootfs.ro" "$NEWROOT/run/rootfs.rw"

mount --move /run/media   "$NEWROOT/run/media"   2>/dev/null || true
mount --move "$SQFS_MNT"  "$NEWROOT/run/rootfs.ro" 2>/dev/null || true

# Clean up initrd pseudo-filesystems
umount /proc 2>/dev/null || true
umount /sys  2>/dev/null || true
umount /dev  2>/dev/null || true

# ── switch_root ──────────────────────────────────────────────────
echo "[initrd] Switching root..."
exec switch_root "$NEWROOT" /init "$@"

# If switch_root fails
echo "[initrd] FATAL: switch_root failed"
mount -t proc proc /proc 2>/dev/null || true
exec /bin/sh
