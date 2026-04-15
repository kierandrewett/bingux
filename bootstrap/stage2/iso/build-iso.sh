#!/bin/bash
# ==========================================================================
# Bingux v2 -- GRUB + SquashFS ISO Builder
# ==========================================================================
# Builds a proper bootable ISO with:
#   - GRUB bootloader
#   - Small initrd (< 10MB) with just busybox + init to find/mount squashfs
#   - Compressed root.sqfs (zstd) containing the full Bingux filesystem
#
# This replaces the old approach of stuffing everything into a 260MB+ gzipped
# cpio initramfs.
#
# ISO layout:
#   isoroot/
#   ├── boot/
#   │   ├── grub/
#   │   │   └── grub.cfg
#   │   ├── vmlinuz
#   │   └── initrd.img
#   └── bingux/
#       └── root.sqfs
#
# Usage: ./build-iso.sh [--output /path/to/output.iso]
# ==========================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ISO_WORK=/tmp/bingux-iso-build
ISO_ROOT="$ISO_WORK/isoroot"
ROOTFS="$ISO_WORK/rootfs"
INITRD_DIR="$ISO_WORK/initrd"
STORE="${BINGUX_STORE:-/tmp/bingux-bootstrap-store}"
CACHE="$ISO_WORK/cache"
OUTPUT="${1:-/tmp/bingux.iso}"
if [ "${1:-}" = "--output" ]; then
    OUTPUT="${2:?--output requires a path}"
fi

# ── Pre-flight checks ────────────────────────────────────────────

check_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "FATAL: required tool '$1' not found"
        echo "  Install with: $2"
        return 1
    fi
}

echo "============================================"
echo "  Bingux v2 ISO Builder (GRUB + SquashFS)"
echo "============================================"
echo ""

MISSING=0
check_tool mksquashfs "sudo dnf install squashfs-tools / sudo apt install squashfs-tools" || MISSING=1

# We need either grub-mkrescue or xorriso
HAS_GRUB_MKRESCUE=0
HAS_XORRISO=0
if command -v grub2-mkrescue >/dev/null 2>&1; then
    GRUB_MKRESCUE=grub2-mkrescue
    HAS_GRUB_MKRESCUE=1
elif command -v grub-mkrescue >/dev/null 2>&1; then
    GRUB_MKRESCUE=grub-mkrescue
    HAS_GRUB_MKRESCUE=1
fi
command -v xorriso >/dev/null 2>&1 && HAS_XORRISO=1

if [ "$HAS_GRUB_MKRESCUE" -eq 0 ] && [ "$HAS_XORRISO" -eq 0 ]; then
    echo "FATAL: need grub-mkrescue (preferred) or xorriso"
    echo "  Install with: sudo dnf install grub2-tools-extra xorriso / sudo apt install grub-common xorriso"
    MISSING=1
fi

check_tool cpio    "coreutils (should be present)" || MISSING=1
check_tool gzip    "gzip (should be present)"      || MISSING=1

[ "$MISSING" -eq 1 ] && exit 1

echo "  Tools OK"
echo ""

# ── Clean slate ──────────────────────────────────────────────────

rm -rf "$ISO_WORK"
mkdir -p "$ISO_ROOT/boot/grub" "$ISO_ROOT/bingux"
mkdir -p "$ROOTFS" "$INITRD_DIR" "$CACHE"

# ── Phase 1: Build Bingux tools ─────────────────────────────────

echo "==> Phase 1: Building Bingux tools"
if [ -f "$ROOT_DIR/Cargo.toml" ]; then
    cargo build --release --manifest-path="$ROOT_DIR/Cargo.toml" 2>&1 | tail -1
    echo "    Tools built"
else
    echo "    WARN: No Cargo.toml found, skipping tool build"
fi

# ── Phase 2: Build packages (if store is empty) ─────────────────

echo ""
echo "==> Phase 2: Gathering packages from $STORE"

if [ ! -d "$STORE" ] || [ -z "$(ls -A "$STORE" 2>/dev/null)" ]; then
    echo "    Store is empty, building packages..."
    mkdir -p "$STORE"

    build_pkg() {
        local name="$1" version="$2" url="$3" script="$4" export="$5"
        local dir="$STORE/${name}-${version}-x86_64-linux"
        [ -d "$dir" ] && return
        echo "    [build] $name $version"
        local work="$ISO_WORK/work/$name/src"
        local pkgdir="$ISO_WORK/work/$name/pkg"
        mkdir -p "$work" "$pkgdir"
        local filename="${url##*/}"
        if [ ! -f "$CACHE/$filename" ]; then
            curl -fSL -o "$CACHE/$filename" "$url" 2>/dev/null || { echo "      WARN: download failed"; return; }
        fi
        case "$filename" in
            *.tar.gz|*.tgz) tar xzf "$CACHE/$filename" -C "$work" ;;
            *.zip) unzip -qo "$CACHE/$filename" -d "$work" 2>/dev/null ;;
            *) cp "$CACHE/$filename" "$work/$filename" ;;
        esac
        SRCDIR="$work" PKGDIR="$pkgdir" bash -c "set -e; $script"
        mkdir -p "$pkgdir/.bpkg"
        cat > "$pkgdir/.bpkg/manifest.toml" << TOML
[package]
name = "$name"
scope = "bingux"
version = "$version"
arch = "x86_64-linux"
description = "$name"
license = "MIT"
[exports]
binaries = ["$export"]
[sandbox]
level = "minimal"
TOML
        cp -a "$pkgdir" "$dir"
    }

    build_pkg "jq" "1.7.1" \
        "https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/jq-linux-amd64" "$PKGDIR/bin/jq"; chmod +x "$PKGDIR/bin/jq"' "bin/jq"
    build_pkg "ripgrep" "14.1.1" \
        "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/ripgrep-14.1.1-x86_64-unknown-linux-musl/rg" "$PKGDIR/bin/rg"; chmod +x "$PKGDIR/bin/rg"' "bin/rg"
    build_pkg "fd" "10.2.0" \
        "https://github.com/sharkdp/fd/releases/download/v10.2.0/fd-v10.2.0-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fd-v10.2.0-x86_64-unknown-linux-musl/fd" "$PKGDIR/bin/fd"; chmod +x "$PKGDIR/bin/fd"' "bin/fd"
    build_pkg "bat" "0.24.0" \
        "https://github.com/sharkdp/bat/releases/download/v0.24.0/bat-v0.24.0-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/bat-v0.24.0-x86_64-unknown-linux-musl/bat" "$PKGDIR/bin/bat"; chmod +x "$PKGDIR/bin/bat"' "bin/bat"
    build_pkg "eza" "0.20.14" \
        "https://github.com/eza-community/eza/releases/download/v0.20.14/eza_x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/eza" "$PKGDIR/bin/eza"; chmod +x "$PKGDIR/bin/eza"' "bin/eza"
    build_pkg "fzf" "0.57.0" \
        "https://github.com/junegunn/fzf/releases/download/v0.57.0/fzf-0.57.0-linux_amd64.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fzf" "$PKGDIR/bin/fzf"; chmod +x "$PKGDIR/bin/fzf"' "bin/fzf"
    build_pkg "dust" "1.1.1" \
        "https://github.com/bootandy/dust/releases/download/v1.1.1/dust-v1.1.1-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/dust-v1.1.1-x86_64-unknown-linux-musl/dust" "$PKGDIR/bin/dust"; chmod +x "$PKGDIR/bin/dust"' "bin/dust"
    build_pkg "delta" "0.18.2" \
        "https://github.com/dandavison/delta/releases/download/0.18.2/delta-0.18.2-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/delta-0.18.2-x86_64-unknown-linux-musl/delta" "$PKGDIR/bin/delta"; chmod +x "$PKGDIR/bin/delta"' "bin/delta"
    build_pkg "zoxide" "0.9.6" \
        "https://github.com/ajeetdsouza/zoxide/releases/download/v0.9.6/zoxide-0.9.6-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/zoxide" "$PKGDIR/bin/zoxide"; chmod +x "$PKGDIR/bin/zoxide"' "bin/zoxide"
fi

PKG_COUNT=$(ls -1 "$STORE" 2>/dev/null | wc -l)
echo "    Store: $PKG_COUNT packages"
ls -1 "$STORE" 2>/dev/null | sed 's/^/      /'

# ── Phase 3: Build the full root filesystem ──────────────────────

echo ""
echo "==> Phase 3: Building root filesystem (for squashfs)"

# Bingux v2 directory layout: /io, /system, /users
# FHS compat symlinks for software that expects /bin, /etc, /usr, etc.
mkdir -p "$ROOTFS"/{io,users/root}
mkdir -p "$ROOTFS"/system/{packages,profiles,config,state/{ephemeral,persistent},kernel/{proc,sys},modules,tmp}
mkdir -p "$ROOTFS"/{bin,sbin,lib64,etc/{ssl/certs},run,tmp,var/{log,run,tmp}}
mkdir -p "$ROOTFS"/usr/{bin,sbin,lib,lib64,share}

# FHS compat symlinks (Bingux v2 model: /io = devices, /system = immutable, /users = home)
ln -sf system/kernel/proc "$ROOTFS/proc" 2>/dev/null || true
ln -sf system/kernel/sys  "$ROOTFS/sys"  2>/dev/null || true
ln -sf io                 "$ROOTFS/dev"  2>/dev/null || true

# ── Busybox ──
if [ -f /tmp/busybox-musl-static ]; then
    cp /tmp/busybox-musl-static "$ROOTFS/bin/busybox"
    echo "    Using self-compiled busybox"
else
    cp /usr/bin/busybox "$ROOTFS/bin/busybox"
    echo "    Using host busybox"
fi

BUSYBOX_CMDS="sh ash cat echo ls mkdir mount umount sleep grep cp rm ln chmod
              clear reboot poweroff init env export wc head tail tr sort uniq
              tee touch mv find xargs sed awk du df uname hostname pwd id whoami
              date vi more less test expr seq basename dirname ip ifconfig
              route ping wget tar gzip gunzip login su passwd
              mount umount mknod dmesg sysctl free top ps kill killall
              insmod modprobe lsmod depmod chown chgrp mke2fs mkfs.ext2
              fdisk sfdisk blkid switch_root pivot_root chroot
              udhcpc udhcpd httpd nc telnet"
for cmd in $BUSYBOX_CMDS; do
    ln -sf busybox "$ROOTFS/bin/$cmd" 2>/dev/null || true
done
# /usr/bin and /usr/sbin compat
for cmd in sh env; do
    ln -sf ../../bin/busybox "$ROOTFS/usr/bin/$cmd" 2>/dev/null || true
done
for cmd in mount umount; do
    ln -sf ../../bin/busybox "$ROOTFS/usr/sbin/$cmd" 2>/dev/null || true
done

# ── Glibc/musl libs for dynamically-linked binaries ──
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 \
           libpthread.so.0 libdl.so.2 libbz2.so.1 libcrypto.so.3 \
           libssl.so.3 libz.so.1; do
    src=""
    for dir in /lib64 /usr/lib64; do
        [ -f "$dir/$lib" ] && src="$dir/$lib" && break
    done
    [ -n "$src" ] && cp -L "$src" "$ROOTFS/lib64/"
done
echo "    Libs: $(ls "$ROOTFS/lib64/" 2>/dev/null | tr '\n' ' ')"

# SSL certs
cp /etc/ssl/certs/ca-bundle.crt "$ROOTFS/etc/ssl/certs/" 2>/dev/null || true

# ── Bingux tools ──
for tool in bpkg bsys-cli bxc-shim bxc-cli; do
    if [ -f "$ROOT_DIR/target/release/$tool" ]; then
        cp "$ROOT_DIR/target/release/$tool" "$ROOTFS/bin/"
        echo "    Included $tool"
    fi
done

# ── Copy package store ──
if [ -d "$STORE" ] && [ "$(ls -A "$STORE" 2>/dev/null)" ]; then
    cp -a "$STORE"/* "$ROOTFS/system/packages/"
    echo "    Copied $PKG_COUNT packages to rootfs"
fi

# ── Create generation profile (dispatch table) ──
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

# ── System config ──
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

# ── /etc template ──
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

# /etc/profile -- shell environment setup
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
export PS1='\[\e[1;36m\]bingux\[\e[0m\]:\[\e[1;34m\]\w\[\e[0m\]\$ '

gen=$(readlink /system/profiles/current 2>/dev/null || echo "?")
pkgs=$(ls /system/packages/ 2>/dev/null | wc -l)
echo "  Bingux v2 | $pkgs packages | generation $gen"
echo ""
PROFILE

# ── /init -- main init script (runs after switch_root) ──
# This is the production bingux-init adapted in-place
cp "$SCRIPT_DIR/bingux-init.sh" "$ROOTFS/init" 2>/dev/null || \
cat > "$ROOTFS/init" << 'INIT'
#!/bin/sh
# Bingux v2 -- Production Init (runs inside squashfs root)
set -e
export PATH="/system/profiles/current/bin:/bin:/sbin:/usr/bin"

# Mount kernel pseudo-filesystems
mount -t proc     proc     /system/kernel/proc 2>/dev/null || mount -t proc proc /proc
mount -t sysfs    sysfs    /system/kernel/sys  2>/dev/null || mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /io 2>/dev/null || mount -t devtmpfs devtmpfs /dev 2>/dev/null || true

# Ephemeral state (tmpfs, cleared every boot)
mount -t tmpfs -o size=256M tmpfs /system/state/ephemeral 2>/dev/null || true
mount -t tmpfs -o size=1500M tmpfs /system/tmp 2>/dev/null || mount -t tmpfs tmpfs /tmp

# devpts / shm
mkdir -p /io/pts /io/shm /dev/pts /dev/shm
mount -t devpts devpts /io/pts  2>/dev/null || mount -t devpts devpts /dev/pts 2>/dev/null || true
mount -t tmpfs  tmpfs  /io/shm  2>/dev/null || mount -t tmpfs tmpfs /dev/shm 2>/dev/null || true

# Environment
export LD_LIBRARY_PATH="/lib64"
export HOME="/users/root"
export TERM=linux
export BPKG_STORE_ROOT="/system/packages"

# Hostname
if [ -f /system/config/system.toml ]; then
    HOSTNAME=$(grep '^hostname' /system/config/system.toml 2>/dev/null | head -1 | sed 's/.*= *"\(.*\)"/\1/')
    [ -n "$HOSTNAME" ] && echo "$HOSTNAME" > /proc/sys/kernel/hostname 2>/dev/null || true
fi

# Networking
ip link set lo up 2>/dev/null || true
for nic in eth0 ens3 enp0s3; do
    if ip link show "$nic" >/dev/null 2>&1; then
        ip link set "$nic" up 2>/dev/null || true
        udhcpc -i "$nic" -q -n 2>/dev/null && break
    fi
done

# Banner
echo ""
echo "  ____  _                        "
echo " | __ )(_)_ __   __ _ _   ___  __"
echo " |  _ \| | '_ \ / _\` | | | \ \/ /"
echo " | |_) | | | | | (_| | |_| |>  < "
echo " |____/|_|_| |_|\__, |\__,_/_/\_\\"
echo "                |___/             "
echo ""
echo "  Bingux v2 -- SquashFS Live"
echo ""
PKGS=$(ls /system/packages/ 2>/dev/null | wc -l)
BINS=$(ls /system/profiles/current/bin/ 2>/dev/null | wc -l)
echo "  Packages: $PKGS"
echo "  Binaries: $BINS"
echo "  Kernel:   $(uname -r)"
echo "  Root:     squashfs + overlay (rw)"
echo ""

# Boot mode: systemd or interactive shell
if grep -q "init.systemd" /proc/cmdline 2>/dev/null; then
    SYSTEMD_BIN=""
    for s in /system/packages/systemd-*/lib/systemd/systemd /usr/lib/systemd/systemd; do
        [ -f "$s" ] && SYSTEMD_BIN="$s" && break
    done
    if [ -n "$SYSTEMD_BIN" ]; then
        echo "[init] Handing off to systemd..."
        mkdir -p /sys/fs/cgroup
        mount -t cgroup2 cgroup2 /sys/fs/cgroup 2>/dev/null || true
        exec "$SYSTEMD_BIN" --system --unit=multi-user.target
    fi
fi

echo "[init] Interactive shell"
echo "  bpkg list   -- list packages"
echo "  bsys apply  -- recompose system"
echo "  bxc run     -- run in sandbox"
echo ""
exec /bin/sh -l
INIT
chmod +x "$ROOTFS/init"

echo "    Root filesystem: $(du -sh "$ROOTFS" | cut -f1) (uncompressed)"

# ── Phase 4: Create squashfs ────────────────────────────────────

echo ""
echo "==> Phase 4: Creating squashfs"

COMP="zstd"
if ! mksquashfs -help 2>&1 | grep -q zstd; then
    echo "    WARN: zstd not supported by mksquashfs, falling back to gzip"
    COMP="gzip"
fi

SQFS_ARGS=(-comp "$COMP" -b 256K -noappend -no-exports -no-recovery)
if [ "$COMP" = "zstd" ]; then
    SQFS_ARGS+=(-Xcompression-level 19)
fi

mksquashfs "$ROOTFS" "$ISO_ROOT/bingux/root.sqfs" \
    "${SQFS_ARGS[@]}" \
    2>&1 | tail -3

echo "    root.sqfs: $(du -h "$ISO_ROOT/bingux/root.sqfs" | cut -f1) ($COMP compressed)"

# ── Phase 5: Build tiny initrd ───────────────────────────────────

echo ""
echo "==> Phase 5: Building initrd (minimal -- just enough to mount squashfs)"

mkdir -p "$INITRD_DIR"/{bin,sbin,lib/modules,dev,proc,sys,run,tmp}

# Busybox -- the only userspace tool we need
if [ -f /tmp/busybox-musl-static ]; then
    cp /tmp/busybox-musl-static "$INITRD_DIR/bin/busybox"
else
    cp /usr/bin/busybox "$INITRD_DIR/bin/busybox"
fi

# Only the commands the initrd init script needs
for cmd in sh mount umount mkdir mknod ls cat echo sleep \
           switch_root modprobe insmod blkid losetup; do
    ln -sf busybox "$INITRD_DIR/bin/$cmd"
done
# Some commands expected in /sbin
for cmd in switch_root modprobe insmod blkid mount umount; do
    ln -sf ../bin/busybox "$INITRD_DIR/sbin/$cmd"
done

# Copy the initrd init script
cp "$SCRIPT_DIR/initrd-init.sh" "$INITRD_DIR/init"
chmod +x "$INITRD_DIR/init"

# ── Kernel modules needed for boot ──
# We need: squashfs, loop, overlay, isofs, sr_mod, cdrom
KVER=$(uname -r)
KMOD_SRC="/lib/modules/$KVER/kernel"

copy_kmod() {
    local modpath
    modpath=$(find "/lib/modules/$KVER" -name "${1}.ko*" 2>/dev/null | head -1)
    if [ -n "$modpath" ]; then
        local relpath="${modpath#/lib/modules/$KVER/}"
        mkdir -p "$INITRD_DIR/lib/modules/$KVER/$(dirname "$relpath")"
        cp "$modpath" "$INITRD_DIR/lib/modules/$KVER/$relpath"
    fi
}

for mod in squashfs loop overlay isofs sr_mod cdrom; do
    copy_kmod "$mod"
done

# Generate modules.dep so modprobe works
if command -v depmod >/dev/null 2>&1; then
    depmod -b "$INITRD_DIR" "$KVER" 2>/dev/null || true
fi

# Pack initrd
(cd "$INITRD_DIR" && find . | cpio -o -H newc 2>/dev/null | gzip -9 > "$ISO_ROOT/boot/initrd.img")

INITRD_SIZE=$(du -h "$ISO_ROOT/boot/initrd.img" | cut -f1)
echo "    initrd.img: $INITRD_SIZE"

# Sanity check: initrd should be < 10MB
INITRD_BYTES=$(stat -c%s "$ISO_ROOT/boot/initrd.img")
if [ "$INITRD_BYTES" -gt 10485760 ]; then
    echo "    WARN: initrd is larger than 10MB ($INITRD_SIZE) -- consider trimming"
else
    echo "    OK: initrd is under 10MB target"
fi

# ── Phase 6: Copy kernel ────────────────────────────────────────

echo ""
echo "==> Phase 6: Copying kernel"

# Try self-compiled kernel first, then host kernel
if [ -f /tmp/linux-kernel-build/linux-6.12.8/arch/x86/boot/bzImage ]; then
    cp /tmp/linux-kernel-build/linux-6.12.8/arch/x86/boot/bzImage "$ISO_ROOT/boot/vmlinuz"
    echo "    Using self-compiled kernel: Linux 6.12.8"
else
    KVER=$(uname -r)
    KERNEL=""
    for k in /boot/vmlinuz-"$KVER" /boot/vmlinuz; do
        [ -f "$k" ] && KERNEL="$k" && break
    done
    if [ -z "$KERNEL" ]; then
        echo "FATAL: no kernel found at /boot/vmlinuz-$KVER"
        exit 1
    fi
    cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"
    echo "    Using host kernel: $KVER ($(du -h "$ISO_ROOT/boot/vmlinuz" | cut -f1))"
fi

# ── Phase 7: GRUB config ────────────────────────────────────────

echo ""
echo "==> Phase 7: Writing GRUB config"

cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'GRUB'
set timeout=3
set default=0

menuentry "Bingux v2" {
    linux /boot/vmlinuz init=/init console=ttyS0 console=tty0
    initrd /boot/initrd.img
}

menuentry "Bingux v2 (systemd)" {
    linux /boot/vmlinuz init=/init console=ttyS0 console=tty0 init.systemd
    initrd /boot/initrd.img
}

menuentry "Bingux v2 (verbose)" {
    linux /boot/vmlinuz init=/init console=ttyS0 console=tty0 loglevel=7
    initrd /boot/initrd.img
}
GRUB

echo "    grub.cfg written"

# ── Phase 8: Build ISO ──────────────────────────────────────────

echo ""
echo "==> Phase 8: Building ISO image"

# Show layout
echo "    ISO layout:"
echo "      isoroot/"
echo "      +-- boot/"
echo "      |   +-- grub/grub.cfg"
echo "      |   +-- vmlinuz        ($(du -h "$ISO_ROOT/boot/vmlinuz" | cut -f1))"
echo "      |   +-- initrd.img     ($INITRD_SIZE)"
echo "      +-- bingux/"
echo "          +-- root.sqfs      ($(du -h "$ISO_ROOT/bingux/root.sqfs" | cut -f1))"
echo ""

if [ "$HAS_GRUB_MKRESCUE" -eq 1 ]; then
    echo "    Using $GRUB_MKRESCUE..."
    $GRUB_MKRESCUE -o "$OUTPUT" "$ISO_ROOT" 2>&1 | tail -5
elif [ "$HAS_XORRISO" -eq 1 ]; then
    echo "    Using xorriso (grub-mkrescue not available)..."
    xorriso -as mkisofs \
        -o "$OUTPUT" \
        -R -J -V "BINGUX_V2" \
        -b boot/grub/grub.cfg \
        -no-emul-boot \
        -boot-load-size 4 \
        -boot-info-table \
        "$ISO_ROOT" 2>&1 | tail -5
fi

if [ ! -f "$OUTPUT" ]; then
    echo "FATAL: ISO build failed"
    exit 1
fi

# ── Done ─────────────────────────────────────────────────────────

ISO_SIZE=$(du -h "$OUTPUT" | cut -f1)
SQFS_SIZE=$(du -h "$ISO_ROOT/bingux/root.sqfs" | cut -f1)

echo ""
echo "============================================"
echo "  ISO built successfully!"
echo "============================================"
echo ""
echo "  Output:     $OUTPUT"
echo "  ISO size:   $ISO_SIZE"
echo "  root.sqfs:  $SQFS_SIZE"
echo "  initrd:     $INITRD_SIZE"
echo "  Packages:   $PKG_COUNT"
echo ""
echo "  Boot with QEMU (from ISO -- proper GRUB boot):"
echo "    qemu-system-x86_64 -enable-kvm -m 1G \\"
echo "      -cdrom $OUTPUT \\"
echo "      -boot d \\"
echo "      -nographic"
echo ""
echo "  Boot with QEMU (direct kernel -- faster, skips GRUB):"
echo "    qemu-system-x86_64 -enable-kvm -m 1G \\"
echo "      -kernel $ISO_ROOT/boot/vmlinuz \\"
echo "      -initrd $ISO_ROOT/boot/initrd.img \\"
echo "      -drive file=$OUTPUT,media=cdrom \\"
echo "      -append 'init=/init console=ttyS0' \\"
echo "      -nographic"
echo ""

# Send notification
if [ -n "${NTFY_API_TOKEN:-}" ]; then
    curl -s -X POST "https://ntfy.drewett.dev/claude" \
        -H "Authorization: Bearer $NTFY_API_TOKEN" \
        -H "Title: Claude Code" \
        -H "Priority: default" \
        -d "Bingux ISO built: $ISO_SIZE (root.sqfs: $SQFS_SIZE, initrd: $INITRD_SIZE, $PKG_COUNT packages)" \
        >/dev/null 2>&1 || true
fi
