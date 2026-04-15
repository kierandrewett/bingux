#!/bin/bash
# ==========================================================================
# Bingux v2 -- SquashFS Boot Builder (direct kernel boot, no GRUB needed)
# ==========================================================================
# Builds:
#   1. root.sqfs  -- compressed root filesystem from the package store
#   2. initrd.img -- tiny initrd (~5MB) that mounts squashfs + overlay
#   3. Boots in QEMU with -kernel/-initrd + squashfs as virtio drive
#
# Usage:
#   ./boot-squashfs.sh           # build + boot
#   ./boot-squashfs.sh --build   # build only (no QEMU)
#   ./boot-squashfs.sh --boot    # boot only (skip build, use cached artifacts)
# ==========================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK=/tmp/bingux-sqfs-boot
ROOTFS="$WORK/rootfs"
INITRD_DIR="$WORK/initrd"
STORE="${BINGUX_STORE:-/tmp/bingux-bootstrap-store}"

SQFS_OUT="$WORK/root.sqfs"
INITRD_OUT="$WORK/initrd.img"
KERNEL_OUT="$WORK/vmlinuz"

MODE="${1:-all}"  # all, --build, --boot

# ── Pre-flight ──────────────────────────────────────────────────

if [ "$MODE" != "--boot" ]; then
    if ! command -v mksquashfs >/dev/null 2>&1; then
        echo "FATAL: mksquashfs not found. Install squashfs-tools."
        exit 1
    fi
    if [ ! -d "$STORE" ] || [ -z "$(ls -A "$STORE" 2>/dev/null)" ]; then
        echo "FATAL: package store at $STORE is empty or missing."
        echo "  Run the bootstrap pipeline first."
        exit 1
    fi
fi

if [ "$MODE" = "--boot" ]; then
    if [ ! -f "$SQFS_OUT" ] || [ ! -f "$INITRD_OUT" ] || [ ! -f "$KERNEL_OUT" ]; then
        echo "FATAL: cached build artifacts not found. Run without --boot first."
        exit 1
    fi
    echo "==> Skipping build, using cached artifacts"
fi

# ── Build Phase ─────────────────────────────────────────────────

if [ "$MODE" != "--boot" ]; then

echo "============================================"
echo "  Bingux v2 SquashFS Boot Builder"
echo "============================================"
echo ""

# Clean
rm -rf "$WORK"
mkdir -p "$ROOTFS" "$INITRD_DIR"

# ── Step 1: Build root filesystem ───────────────────────────────

echo "==> Step 1: Building root filesystem"

# Bingux v2 directory layout
mkdir -p "$ROOTFS"/{io,users/root}
mkdir -p "$ROOTFS"/system/{packages,profiles,config,state/{ephemeral,persistent},kernel/{proc,sys},modules,tmp}
mkdir -p "$ROOTFS"/{bin,sbin,lib64,etc/{ssl/certs},run,tmp,var/{log,run,tmp}}
mkdir -p "$ROOTFS"/usr/{bin,sbin,lib,lib64,share}

# FHS compat symlinks
ln -sf system/kernel/proc "$ROOTFS/proc" 2>/dev/null || true
ln -sf system/kernel/sys  "$ROOTFS/sys"  2>/dev/null || true
ln -sf io                 "$ROOTFS/dev"  2>/dev/null || true

# Busybox (static)
cp /tmp/busybox-musl-static "$ROOTFS/bin/busybox"
echo "    Busybox: static musl ($(du -h "$ROOTFS/bin/busybox" | cut -f1))"

BUSYBOX_CMDS="sh ash cat echo ls mkdir mount umount sleep grep cp rm ln chmod
              clear reboot poweroff init env export wc head tail tr sort uniq
              tee touch mv find xargs sed awk du df uname hostname pwd id whoami
              date vi more less test expr seq basename dirname ip ifconfig
              route ping wget tar gzip gunzip login su passwd
              mount umount mknod dmesg sysctl free top ps kill killall
              insmod modprobe lsmod depmod chown chgrp
              fdisk sfdisk blkid switch_root pivot_root chroot
              udhcpc httpd nc mkfs.ext2"
for cmd in $BUSYBOX_CMDS; do
    ln -sf busybox "$ROOTFS/bin/$cmd" 2>/dev/null || true
done
for cmd in sh env; do
    ln -sf ../../bin/busybox "$ROOTFS/usr/bin/$cmd" 2>/dev/null || true
done
for cmd in mount umount; do
    ln -sf ../../bin/busybox "$ROOTFS/usr/sbin/$cmd" 2>/dev/null || true
done

# Glibc libs for dynamically-linked binaries
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 \
           libpthread.so.0 libdl.so.2 libbz2.so.1 libcrypto.so.3 \
           libssl.so.3 libz.so.1 libresolv.so.2 libnss_dns.so.2 \
           libnss_files.so.2 librt.so.1; do
    for dir in /lib64 /usr/lib64; do
        [ -f "$dir/$lib" ] && cp -L "$dir/$lib" "$ROOTFS/lib64/" && break
    done
done
echo "    Libs: $(ls "$ROOTFS/lib64/" | wc -l) shared libraries"

# SSL certs
cp /etc/ssl/certs/ca-bundle.crt "$ROOTFS/etc/ssl/certs/" 2>/dev/null || true

# Bingux tools
for tool in bpkg bsys-cli bxc-shim bxc-cli; do
    if [ -f "$ROOT_DIR/target/release/$tool" ]; then
        cp "$ROOT_DIR/target/release/$tool" "$ROOTFS/bin/"
        echo "    Included $tool"
    fi
done

# Copy package store
if [ -d "$STORE" ] && [ "$(ls -A "$STORE" 2>/dev/null)" ]; then
    cp -a "$STORE"/* "$ROOTFS/system/packages/"
    PKG_COUNT=$(ls -1 "$ROOTFS/system/packages/" | wc -l)
    echo "    Copied $PKG_COUNT packages"
fi

# Create generation profile (symlink dispatch table)
PROFILE_DIR="$ROOTFS/system/profiles/1"
PROFILE_BIN="$PROFILE_DIR/bin"
mkdir -p "$PROFILE_BIN" "$PROFILE_DIR/lib" "$PROFILE_DIR/share"

for pkg_dir in "$ROOTFS/system/packages"/*/; do
    [ -d "$pkg_dir/bin" ] || continue
    for binary in "$pkg_dir/bin"/*; do
        [ -f "$binary" ] || continue
        bin_name=$(basename "$binary")
        pkg_name=$(basename "$pkg_dir")
        ln -sf "/system/packages/$pkg_name/bin/$bin_name" "$PROFILE_BIN/$bin_name"
    done
done
ln -sf 1 "$ROOTFS/system/profiles/current"
echo "    Profile: $(ls "$PROFILE_BIN" 2>/dev/null | wc -l) binaries"

# System config
cat > "$ROOTFS/system/config/system.toml" << 'SYSCONF'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["jq", "ripgrep", "fd", "bat", "eza", "delta", "zoxide", "fzf", "dust"]

[services]
enable = []
SYSCONF

# /etc files
cat > "$ROOTFS/etc/os-release" << 'OSREL'
NAME="Bingux"
ID=bingux
VERSION="0.2.0"
PRETTY_NAME="Bingux v2"
HOME_URL="https://github.com/kierandrewett/bingux"
OSREL

echo "bingux" > "$ROOTFS/etc/hostname"

cat > "$ROOTFS/etc/passwd" << 'P'
root:x:0:0:root:/users/root:/bin/sh
nobody:x:65534:65534:Nobody:/:/sbin/nologin
bingux:x:1000:1000:Bingux User:/users/bingux:/bin/sh
P

cat > "$ROOTFS/etc/group" << 'G'
root:x:0:
wheel:x:10:bingux
nobody:x:65534:
bingux:x:1000:
G

cat > "$ROOTFS/etc/shadow" << 'S'
root::0:0:99999:7:::
bingux::0:0:99999:7:::
S
chmod 600 "$ROOTFS/etc/shadow"

cat > "$ROOTFS/etc/profile" << 'PROFILE'
export PATH="/system/profiles/current/bin:/bin:/sbin:/usr/bin:/usr/sbin"
export LD_LIBRARY_PATH="/lib64:/usr/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_CONFIG_PATH="/system/config/system.toml"
export BSYS_PROFILES_ROOT="/system/profiles"
export BSYS_PACKAGES_ROOT="/system/packages"
export HOME="/users/root"
export TERM="linux"
export SSL_CERT_FILE="/etc/ssl/certs/ca-bundle.crt"
export PS1='bingux:\w\$ '

gen=$(readlink /system/profiles/current 2>/dev/null || echo "?")
pkgs=$(ls /system/packages/ 2>/dev/null | wc -l)
echo "  Bingux v2 | $pkgs packages | generation $gen"
echo ""
PROFILE

# /init -- production init (runs after switch_root from the squashfs root)
cp "$SCRIPT_DIR/bingux-init.sh" "$ROOTFS/init"
chmod +x "$ROOTFS/init"

echo "    Root filesystem: $(du -sh "$ROOTFS" | cut -f1) (uncompressed)"
echo ""

# ── Step 2: Create squashfs ────────────────────────────────────

echo "==> Step 2: Creating squashfs"

COMP="zstd"
if ! mksquashfs -help 2>&1 | grep -q zstd; then
    echo "    WARN: zstd not supported, falling back to gzip"
    COMP="gzip"
fi

SQFS_ARGS=(-comp "$COMP" -b 256K -noappend -no-exports -no-recovery -quiet)
if [ "$COMP" = "zstd" ]; then
    SQFS_ARGS+=(-Xcompression-level 15)
fi

mksquashfs "$ROOTFS" "$SQFS_OUT" "${SQFS_ARGS[@]}" 2>&1 | tail -3

echo "    root.sqfs: $(du -h "$SQFS_OUT" | cut -f1) ($COMP compressed)"
echo ""

# ── Step 3: Build tiny initrd ──────────────────────────────────

echo "==> Step 3: Building initrd"

mkdir -p "$INITRD_DIR"/{bin,sbin,lib/modules,dev,proc,sys,run,tmp,mnt}

# Busybox (static) -- only tool needed in initrd
cp /tmp/busybox-musl-static "$INITRD_DIR/bin/busybox"

for cmd in sh mount umount mkdir mknod ls cat echo sleep \
           switch_root modprobe insmod blkid losetup; do
    ln -sf busybox "$INITRD_DIR/bin/$cmd"
done
for cmd in switch_root modprobe insmod blkid mount umount; do
    ln -sf ../bin/busybox "$INITRD_DIR/sbin/$cmd"
done

# Kernel modules needed for boot: squashfs, loop, overlay
KVER=$(uname -r)
copy_kmod() {
    local modpath
    modpath=$(find "/lib/modules/$KVER" -name "${1}.ko*" 2>/dev/null | head -1)
    if [ -n "$modpath" ]; then
        local relpath="${modpath#/lib/modules/$KVER/}"
        mkdir -p "$INITRD_DIR/lib/modules/$KVER/$(dirname "$relpath")"
        cp "$modpath" "$INITRD_DIR/lib/modules/$KVER/$relpath"
        echo "    Module: $1"
    else
        echo "    Module: $1 (not found -- may be built-in)"
    fi
}

for mod in squashfs loop overlay; do
    copy_kmod "$mod"
done

# Generate modules.dep
if command -v depmod >/dev/null 2>&1; then
    depmod -b "$INITRD_DIR" "$KVER" 2>/dev/null || true
fi

# Initrd init script -- finds squashfs on /dev/vda, mounts it with overlay
cat > "$INITRD_DIR/init" << 'INITRD_INIT'
#!/bin/sh
# Bingux v2 -- Initrd init (finds squashfs on virtio drive, mounts with overlay)
set -e
export PATH="/bin:/sbin"

mount -t proc     proc     /proc
mount -t sysfs    sysfs    /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs    tmpfs    /run

[ -e /dev/null ]    || mknod /dev/null    c 1 3
[ -e /dev/console ] || mknod /dev/console c 5 1

echo "[initrd] Bingux v2 early init"

# Load modules
for mod in squashfs loop overlay; do
    modprobe "$mod" 2>/dev/null || true
done

# Wait briefly for devices
sleep 1

# ── Find the squashfs image ──────────────────────────────────
SQFS_DEV=""

# Method 1: Direct virtio drive (squashfs mounted directly as block device)
# When booted with: -drive file=root.sqfs,format=raw,readonly=on,if=virtio
# The squashfs IS the block device -- mount it directly
for dev in /dev/vda /dev/vdb /dev/sda /dev/sdb; do
    if [ -b "$dev" ]; then
        echo "[initrd] Trying $dev as squashfs..."
        mkdir -p /run/rootfs.ro
        if mount -t squashfs -o ro "$dev" /run/rootfs.ro 2>/dev/null; then
            echo "[initrd] Mounted squashfs from $dev"
            SQFS_DEV="$dev"
            break
        fi
        # Maybe it's a filesystem containing root.sqfs
        mkdir -p /run/media
        if mount -o ro "$dev" /run/media 2>/dev/null; then
            if [ -f /run/media/bingux/root.sqfs ]; then
                echo "[initrd] Found root.sqfs on $dev"
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

if [ -z "$SQFS_DEV" ]; then
    echo "[initrd] FATAL: could not find squashfs root on any device"
    echo "[initrd] Block devices:"
    ls -la /dev/vd* /dev/sd* /dev/sr* 2>/dev/null || echo "  (none)"
    echo "[initrd] Dropping to shell"
    exec /bin/sh
fi

# ── Set up overlay (rw layer on tmpfs over ro squashfs) ───────
echo "[initrd] Setting up overlay..."
mkdir -p /run/rootfs.rw/upper /run/rootfs.rw/work /run/newroot
mount -t tmpfs -o size=512M tmpfs /run/rootfs.rw
mkdir -p /run/rootfs.rw/upper /run/rootfs.rw/work
mount -t overlay overlay \
    -o "lowerdir=/run/rootfs.ro,upperdir=/run/rootfs.rw/upper,workdir=/run/rootfs.rw/work" \
    /run/newroot

# Prepare for switch_root
mkdir -p /run/newroot/run/rootfs.ro /run/newroot/run/rootfs.rw
mount --move /run/rootfs.ro /run/newroot/run/rootfs.ro 2>/dev/null || true

# Clean up initrd mounts
umount /proc 2>/dev/null || true
umount /sys  2>/dev/null || true
umount /dev  2>/dev/null || true

echo "[initrd] Switching root..."
exec switch_root /run/newroot /init
echo "[initrd] FATAL: switch_root failed"
mount -t proc proc /proc 2>/dev/null || true
exec /bin/sh
INITRD_INIT
chmod +x "$INITRD_DIR/init"

# Pack initrd
(cd "$INITRD_DIR" && find . | cpio -o -H newc 2>/dev/null | gzip -9 > "$INITRD_OUT")

INITRD_SIZE=$(du -h "$INITRD_OUT" | cut -f1)
echo "    initrd.img: $INITRD_SIZE"

INITRD_BYTES=$(stat -c%s "$INITRD_OUT")
if [ "$INITRD_BYTES" -gt 10485760 ]; then
    echo "    WARN: initrd is larger than 10MB ($INITRD_SIZE)"
else
    echo "    OK: initrd under 10MB target"
fi
echo ""

# ── Step 4: Copy kernel ───────────────────────────────────────

echo "==> Step 4: Copying kernel"
KVER=$(uname -r)
KERNEL=""
for k in /boot/vmlinuz-"$KVER" /boot/vmlinuz; do
    [ -f "$k" ] && KERNEL="$k" && break
done
if [ -z "$KERNEL" ]; then
    echo "FATAL: no kernel found"
    exit 1
fi
cp "$KERNEL" "$KERNEL_OUT"
echo "    Kernel: $KVER ($(du -h "$KERNEL_OUT" | cut -f1))"
echo ""

# ── Summary ───────────────────────────────────────────────────

echo "============================================"
echo "  Build complete!"
echo "============================================"
echo ""
echo "  root.sqfs:  $(du -h "$SQFS_OUT" | cut -f1)"
echo "  initrd.img: $INITRD_SIZE"
echo "  vmlinuz:    $(du -h "$KERNEL_OUT" | cut -f1)"
echo "  Packages:   $(ls -1 "$ROOTFS/system/packages/" | wc -l)"
echo "  Binaries:   $(ls -1 "$ROOTFS/system/profiles/1/bin/" 2>/dev/null | wc -l)"
echo ""

fi  # end build phase

# ── Boot Phase ──────────────────────────────────────────────────

if [ "$MODE" = "--build" ]; then
    echo "Build-only mode. To boot:"
    echo "  qemu-system-x86_64 -enable-kvm -m 1G \\"
    echo "    -kernel $KERNEL_OUT \\"
    echo "    -initrd $INITRD_OUT \\"
    echo "    -drive file=$SQFS_OUT,format=raw,readonly=on,if=virtio \\"
    echo "    -append 'init=/init console=ttyS0' \\"
    echo "    -nographic"
    exit 0
fi

echo "==> Booting in QEMU..."
echo "    (Ctrl-A X to exit)"
echo ""

exec qemu-system-x86_64 \
    -enable-kvm \
    -m 1G \
    -kernel "$KERNEL_OUT" \
    -initrd "$INITRD_OUT" \
    -drive "file=$SQFS_OUT,format=raw,readonly=on,if=virtio" \
    -append "init=/init console=ttyS0 loglevel=4" \
    -nographic \
    -no-reboot
