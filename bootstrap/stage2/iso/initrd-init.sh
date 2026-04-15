#!/bin/sh
# Bingux v2 -- Minimal initrd init
# Purpose: find squashfs root (virtio drive, CD-ROM, or disk), mount with overlay,
#          then switch_root into the real root filesystem.
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

echo "[initrd] Bingux v2 early init"

# ── Load necessary kernel modules ────────────────────────────────
for mod in squashfs loop isofs sr_mod cdrom overlay; do
    modprobe "$mod" 2>/dev/null || true
done

# Wait a moment for devices to settle
sleep 1

# ── Find and mount the squashfs root ────────────────────────────
SQFS_DEV=""

# Method 1: Direct virtio/disk drive (squashfs IS the block device)
# Used with: -drive file=root.sqfs,format=raw,readonly=on,if=virtio
for dev in /dev/vda /dev/vdb /dev/sda /dev/sdb; do
    if [ -b "$dev" ]; then
        echo "[initrd] Trying $dev as squashfs..."
        mkdir -p /run/rootfs.ro
        if mount -t squashfs -o ro "$dev" /run/rootfs.ro 2>/dev/null; then
            echo "[initrd] Mounted squashfs from $dev"
            SQFS_DEV="$dev"
            break
        fi
        # Maybe it's a filesystem containing /bingux/root.sqfs
        mkdir -p /run/media
        if mount -o ro "$dev" /run/media 2>/dev/null; then
            if [ -f /run/media/bingux/root.sqfs ]; then
                echo "[initrd] Found root.sqfs on $dev"
                mkdir -p /run/rootfs.ro
                mount -t squashfs -o ro,loop /run/media/bingux/root.sqfs /run/rootfs.ro
                SQFS_DEV="$dev"
                break
            fi
            umount /run/media 2>/dev/null || true
        fi
    fi
done

# Method 2: CD-ROM (ISO boot)
if [ -z "$SQFS_DEV" ]; then
    for dev in /dev/sr0 /dev/sr1 /dev/cdrom; do
        if [ -b "$dev" ]; then
            mkdir -p /run/media
            if mount -o ro "$dev" /run/media 2>/dev/null; then
                if [ -f /run/media/bingux/root.sqfs ]; then
                    echo "[initrd] Found root.sqfs on $dev (CD-ROM)"
                    mkdir -p /run/rootfs.ro
                    mount -t squashfs -o ro,loop /run/media/bingux/root.sqfs /run/rootfs.ro
                    SQFS_DEV="$dev"
                    break
                fi
                umount /run/media 2>/dev/null || true
            fi
        fi
    done
fi

# Method 3: Scan all block devices
if [ -z "$SQFS_DEV" ]; then
    for blockdev in /sys/block/*/; do
        devname="/dev/$(basename "$blockdev")"
        [ -b "$devname" ] || continue
        mkdir -p /run/rootfs.ro
        if mount -t squashfs -o ro "$devname" /run/rootfs.ro 2>/dev/null; then
            echo "[initrd] Mounted squashfs from $devname"
            SQFS_DEV="$devname"
            break
        fi
        mkdir -p /run/media
        if mount -o ro "$devname" /run/media 2>/dev/null; then
            if [ -f /run/media/bingux/root.sqfs ]; then
                echo "[initrd] Found root.sqfs on $devname"
                mount -t squashfs -o ro,loop /run/media/bingux/root.sqfs /run/rootfs.ro
                SQFS_DEV="$devname"
                break
            fi
            umount /run/media 2>/dev/null || true
        fi
        # Check partitions
        for part in "$blockdev"/*/; do
            partname="/dev/$(basename "$part")"
            [ -b "$partname" ] || continue
            if mount -t squashfs -o ro "$partname" /run/rootfs.ro 2>/dev/null; then
                echo "[initrd] Mounted squashfs from $partname"
                SQFS_DEV="$partname"
                break 2
            fi
            if mount -o ro "$partname" /run/media 2>/dev/null; then
                if [ -f /run/media/bingux/root.sqfs ]; then
                    echo "[initrd] Found root.sqfs on $partname"
                    mount -t squashfs -o ro,loop /run/media/bingux/root.sqfs /run/rootfs.ro
                    SQFS_DEV="$partname"
                    break 2
                fi
                umount /run/media 2>/dev/null || true
            fi
        done
    done
fi

if [ -z "$SQFS_DEV" ]; then
    echo "[initrd] FATAL: could not find squashfs root on any device"
    echo "[initrd] Available block devices:"
    ls -la /dev/sd* /dev/vd* /dev/sr* /dev/nvme* 2>/dev/null || echo "  (none)"
    echo "[initrd] Dropping to emergency shell"
    exec /bin/sh
fi

# ── Set up overlay (rw layer on tmpfs over ro squashfs) ─────────
echo "[initrd] Setting up overlay (rw layer on tmpfs)..."
mkdir -p /run/rootfs.rw/upper /run/rootfs.rw/work /run/newroot
mount -t tmpfs -o size=512M tmpfs /run/rootfs.rw
mkdir -p /run/rootfs.rw/upper /run/rootfs.rw/work
mount -t overlay overlay \
    -o "lowerdir=/run/rootfs.ro,upperdir=/run/rootfs.rw/upper,workdir=/run/rootfs.rw/work" \
    /run/newroot

# ── Prepare for switch_root ─────────────────────────────────────
# Move mounted filesystems into the new root so they persist
mkdir -p /run/newroot/run/media /run/newroot/run/rootfs.ro /run/newroot/run/rootfs.rw

mount --move /run/rootfs.ro /run/newroot/run/rootfs.ro 2>/dev/null || true
mount --move /run/media     /run/newroot/run/media      2>/dev/null || true

# Clean up initrd pseudo-filesystems
umount /proc 2>/dev/null || true
umount /sys  2>/dev/null || true
umount /dev  2>/dev/null || true

# ── switch_root ──────────────────────────────────────────────────
echo "[initrd] Switching root..."
exec switch_root /run/newroot /init "$@"

# If switch_root fails
echo "[initrd] FATAL: switch_root failed"
mount -t proc proc /proc 2>/dev/null || true
exec /bin/sh
