#!/bin/bash
# Build a bootable Bingux ISO with real packages from the internet.
# Uses the host kernel + a cpio initramfs with bingux tools and real packages.
# No root required.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
ISO_WORK=/tmp/bingux-iso-build
ISO_ROOT="$ISO_WORK/isoroot"
STORE="$ISO_WORK/store"
CACHE="$ISO_WORK/cache"
KERNEL=$(ls /boot/vmlinuz-$(uname -r))

rm -rf "$ISO_WORK"
mkdir -p "$ISO_ROOT/boot/grub" "$STORE" "$CACHE"

echo "============================================"
echo "  Bingux Live ISO Builder"
echo "============================================"
echo ""

# --- Phase 1: Build Bingux tools ---
echo "==> Phase 1: Building Bingux tools"
cargo build --release --manifest-path="$ROOT_DIR/Cargo.toml" 2>&1 | tail -1
echo "    Tools ready: bpkg, bsys-cli, bxc-shim, bxc-cli"

# --- Phase 2: Build real packages ---
echo ""
echo "==> Phase 2: Building real packages"

# Helper function: build a package from a recipe
# Usage: build_package <name> <version> <desc> <source_url> <package_script> <exports...>
build_package() {
    local name="$1" version="$2" desc="$3" source_url="$4" package_script="$5"
    shift 5
    local exports=("$@")

    local pkg_dir_name="${name}-${version}-x86_64-linux"
    if [ -d "$STORE/$pkg_dir_name" ]; then
        echo "    [skip] $name $version (already built)"
        return
    fi

    echo "    [build] $name $version"

    local work="$ISO_WORK/work/$name"
    local srcdir="$work/src"
    local pkgdir="$work/pkg"
    mkdir -p "$srcdir" "$pkgdir"

    # Download source
    local filename="${source_url##*/}"
    if [ ! -f "$CACHE/$filename" ]; then
        echo "      Downloading $filename..."
        curl -fSL -o "$CACHE/$filename" "$source_url"
    else
        echo "      Using cached $filename"
    fi

    # Extract or copy source to srcdir
    case "$filename" in
        *.tar.gz|*.tgz)
            tar xzf "$CACHE/$filename" -C "$srcdir"
            ;;
        *)
            cp "$CACHE/$filename" "$srcdir/$filename"
            ;;
    esac

    # Run package() script
    SRCDIR="$srcdir" BUILDDIR="$work/build" PKGDIR="$pkgdir" \
        bash -c "set -e; $package_script"

    # Write manifest
    local meta="$pkgdir/.bpkg"
    mkdir -p "$meta"

    local exports_toml=""
    for exp in "${exports[@]}"; do
        exports_toml+="\"$exp\", "
    done

    cat > "$meta/manifest.toml" << MANIFEST
[package]
name = "$name"
scope = "bingux"
version = "$version"
arch = "x86_64-linux"
description = "$desc"
license = "MIT"

[exports]
binaries = [${exports_toml%, }]

[sandbox]
level = "minimal"
MANIFEST

    # Install to store
    cp -a "$pkgdir" "$STORE/$pkg_dir_name"
    echo "      Installed to $pkg_dir_name"
}

# jq 1.7.1
build_package "jq" "1.7.1" "Command-line JSON processor" \
    "https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/jq-linux-amd64" "$PKGDIR/bin/jq"; chmod +x "$PKGDIR/bin/jq"' \
    "bin/jq"

# ripgrep 14.1.1
build_package "ripgrep" "14.1.1" "Fast regex search tool" \
    "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/ripgrep-14.1.1-x86_64-unknown-linux-musl/rg" "$PKGDIR/bin/rg"; chmod +x "$PKGDIR/bin/rg"' \
    "bin/rg"

# fd 10.2.0
build_package "fd" "10.2.0" "Fast find alternative" \
    "https://github.com/sharkdp/fd/releases/download/v10.2.0/fd-v10.2.0-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fd-v10.2.0-x86_64-unknown-linux-musl/fd" "$PKGDIR/bin/fd"; chmod +x "$PKGDIR/bin/fd"' \
    "bin/fd"

# bat 0.24.0
build_package "bat" "0.24.0" "Cat clone with syntax highlighting" \
    "https://github.com/sharkdp/bat/releases/download/v0.24.0/bat-v0.24.0-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/bat-v0.24.0-x86_64-unknown-linux-musl/bat" "$PKGDIR/bin/bat"; chmod +x "$PKGDIR/bin/bat"' \
    "bin/bat"

# eza 0.20.14
build_package "eza" "0.20.14" "Modern ls replacement" \
    "https://github.com/eza-community/eza/releases/download/v0.20.14/eza_x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/eza" "$PKGDIR/bin/eza"; chmod +x "$PKGDIR/bin/eza"' \
    "bin/eza"

echo ""
echo "    All packages built:"
ls -1 "$STORE/" | sed 's/^/      /'

# --- Phase 3: Build initramfs ---
echo ""
echo "==> Phase 3: Building initramfs"

INITRD="$ISO_WORK/initramfs"
mkdir -p "$INITRD"/{bin,sbin,lib64,dev,proc,sys,run,tmp,etc}
mkdir -p "$INITRD/system"/{packages,profiles,config,state}

# Copy kernel
cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"

# Busybox + symlinks
cp /usr/bin/busybox "$INITRD/bin/"
BUSYBOX_CMDS="sh ash cat echo ls mkdir mount umount sleep grep cp rm ln chmod
              clear reboot poweroff init env export wc head tail tr sort uniq
              tee touch mv find xargs sed awk du df uname hostname pwd id whoami
              date vi more less test expr seq basename dirname"
for cmd in $BUSYBOX_CMDS; do
    ln -sf busybox "$INITRD/bin/$cmd"
done

# Bingux tools
for tool in bpkg bsys-cli bxc-shim bxc-cli; do
    if [ -f "$ROOT_DIR/target/release/$tool" ]; then
        cp "$ROOT_DIR/target/release/$tool" "$INITRD/bin/"
        echo "    Included $tool"
    else
        echo "    WARN: $tool not found in target/release/"
    fi
done

# Glibc libs needed by bpkg and other dynamically linked binaries
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libpthread.so.0 libdl.so.2; do
    if [ -f "/lib64/$lib" ]; then
        cp -L "/lib64/$lib" "$INITRD/lib64/"
    elif [ -f "/usr/lib64/$lib" ]; then
        cp -L "/usr/lib64/$lib" "$INITRD/lib64/"
    fi
done
echo "    Libs: $(ls "$INITRD/lib64/" | tr '\n' ' ')"

# Copy all packages into the initramfs store
cp -a "$STORE"/* "$INITRD/system/packages/"
echo "    Copied $(ls -1 "$STORE" | wc -l) packages to initramfs"

# Create generation profile with dispatch table (symlinks to package binaries)
PROFILE_DIR="$INITRD/system/profiles/default"
PROFILE_BIN="$PROFILE_DIR/bin"
mkdir -p "$PROFILE_BIN"

echo "    Creating generation dispatch table..."
for pkg_dir in "$INITRD/system/packages"/*/; do
    manifest="$pkg_dir/.bpkg/manifest.toml"
    [ -f "$manifest" ] || continue

    # Find binaries in the package
    if [ -d "$pkg_dir/bin" ]; then
        for binary in "$pkg_dir/bin"/*; do
            [ -f "$binary" ] || continue
            local_name="$(basename "$binary")"
            # Create symlink from profile bin to package binary
            # Use path relative to initramfs root
            pkg_store_path="/system/packages/$(basename "$pkg_dir")/bin/$local_name"
            ln -sf "$pkg_store_path" "$PROFILE_BIN/$local_name"
            echo "      $local_name -> $pkg_store_path"
        done
    fi
done

# System config
cat > "$INITRD/system/config/system.toml" << 'SYSCONF'
[system]
hostname = "bingux-live"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["jq", "ripgrep", "fd", "bat", "eza"]

[services]
enable = []
SYSCONF

# /etc/profile to set up PATH
cat > "$INITRD/etc/profile" << 'PROFILE'
export PATH="/system/profiles/default/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export HOME="/tmp"
export TERM="linux"
export PS1='\[\033[1;36m\]bingux\[\033[0m\]:\[\033[1;34m\]\w\[\033[0m\]\$ '
PROFILE

# Init script
cat > "$INITRD/init" << 'INIT'
#!/bin/sh
# Bingux Live Init
export PATH="/system/profiles/default/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export HOME="/tmp"
export TERM="linux"

# Mount essential filesystems
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp
mount -t tmpfs tmpfs /run

# Banner
echo ""
echo "  ____  _                       "
echo " | __ )(_)_ __   __ _ _   ___  _"
echo " |  _ \| | '_ \ / _\` | | | \ \/ /"
echo " | |_) | | | | | (_| | |_| |>  < "
echo " |____/|_|_| |_|\__, |\__,_/_/\_\\"
echo "                |___/            "
echo ""
echo "  Bingux Live Environment"
echo "  ======================"
echo ""
echo "  Hello from Bingux!"
echo ""

# Show installed packages
echo "[init] Installed packages:"
bpkg list 2>&1 || echo "  (bpkg list failed)"
echo ""

# Verify tools
echo "[init] Tool verification:"
for cmd in jq rg fd bat eza; do
    if command -v $cmd >/dev/null 2>&1; then
        version=$($cmd --version 2>&1 | head -1)
        echo "  OK  $cmd: $version"
    else
        echo "  FAIL $cmd: not found"
    fi
done
echo ""

echo "[init] Type 'bpkg list' to see packages, or use jq/rg/fd/bat/eza."
echo "[init] Ready."
echo ""

# Launch interactive shell
exec /bin/sh -l
INIT
chmod +x "$INITRD/init"

# --- Phase 4: Create ISO ---
echo ""
echo "==> Phase 4: Creating ISO"

# Create initramfs cpio archive
(cd "$INITRD" && find . | cpio -o -H newc 2>/dev/null | gzip > "$ISO_ROOT/boot/initramfs.img")
echo "    initramfs: $(du -h "$ISO_ROOT/boot/initramfs.img" | cut -f1)"

# GRUB config
cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'GRUB'
set timeout=3
set default=0

menuentry "Bingux Live" {
    linux /boot/vmlinuz init=/init console=ttyS0 quiet
    initrd /boot/initramfs.img
}

menuentry "Bingux Live (verbose)" {
    linux /boot/vmlinuz init=/init console=ttyS0
    initrd /boot/initramfs.img
}
GRUB

# Create ISO image
genisoimage -o /tmp/bingux-live.iso -R -J -V "BINGUX_LIVE" \
    -b boot/grub/grub.cfg -no-emul-boot \
    "$ISO_ROOT" 2>/dev/null

echo ""
echo "============================================"
echo "  ISO built: /tmp/bingux-live.iso"
echo "  Size: $(du -h /tmp/bingux-live.iso | cut -f1)"
echo "============================================"
echo ""
echo "Boot with:"
echo "  qemu-system-x86_64 -enable-kvm -m 512M \\"
echo "    -kernel $ISO_ROOT/boot/vmlinuz \\"
echo "    -initrd $ISO_ROOT/boot/initramfs.img \\"
echo "    -append 'init=/init console=ttyS0' \\"
echo "    -nographic"
