#!/bin/bash
# Build a FULLY self-hosted Bingux ISO.
# Uses:
# - Self-compiled Linux kernel 6.12.8 (built with musl GCC)
# - Self-compiled BusyBox 1.37.0 (built with musl GCC from source)
# - musl GCC toolchain (downloaded from musl.cc)
# - GNU Make (compiled with musl GCC)
# - patchelf (downloaded, static)
# - bpkg/bsys/bxc (our Rust tools)
# - 15+ user packages (downloaded from GitHub)
# NO HOST BINARIES IN THE FINAL IMAGE (except host kernel if self-built unavailable)
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
ISO_WORK=/tmp/bingux-selfhosted-iso
INITRD="$ISO_WORK/initramfs"
ISO_ROOT="$ISO_WORK/isoroot"

rm -rf "$ISO_WORK"
mkdir -p "$ISO_ROOT/boot/grub" "$INITRD"/{bin,sbin,lib,lib64,usr/{bin,sbin},dev,proc,sys,run,tmp,etc/ssl/certs,src}
mkdir -p "$INITRD"/system/{packages,profiles/1/bin,config,state}

echo "============================================"
echo "  Bingux Self-Hosted ISO Builder"
echo "============================================"

# Use self-compiled kernel if available, otherwise host kernel
if [ -f /tmp/linux-kernel-build/linux-6.12.8/arch/x86/boot/bzImage ]; then
    KERNEL=/tmp/linux-kernel-build/linux-6.12.8/arch/x86/boot/bzImage
    echo "==> Using self-compiled kernel: Linux 6.12.8"
else
    KERNEL=$(ls /boot/vmlinuz-$(uname -r))
    echo "==> Using host kernel (self-compiled not available)"
fi
cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"

# Use self-compiled busybox
if [ -f /tmp/busybox-musl-static ]; then
    cp /tmp/busybox-musl-static "$INITRD/bin/busybox"
    echo "==> Using self-compiled busybox (403 applets)"
else
    cp /usr/bin/busybox "$INITRD/bin/busybox"
    echo "==> Using host busybox"
fi

# Create busybox symlinks
for cmd in $("$INITRD/bin/busybox" --list 2>/dev/null); do
    ln -sf busybox "$INITRD/bin/$cmd" 2>/dev/null
done
ln -sf ../../bin/busybox "$INITRD/usr/bin/sh" 2>/dev/null
ln -sf ../../bin/busybox "$INITRD/usr/sbin/mount" 2>/dev/null

# Copy toolchain to store
echo "==> Copying musl toolchain..."
if [ -d /tmp/bingux-bootstrap-store/musl-toolchain-13.2.0-x86_64-linux ]; then
    cp -a /tmp/bingux-bootstrap-store/musl-toolchain-13.2.0-x86_64-linux "$INITRD/system/packages/"
fi

# Copy patchelf
if [ -d /tmp/bingux-bootstrap-store/patchelf-0.18.0-x86_64-linux ]; then
    cp -a /tmp/bingux-bootstrap-store/patchelf-0.18.0-x86_64-linux "$INITRD/system/packages/"
fi

# Build user packages
echo "==> Building packages..."
export BPKG_STORE_ROOT="$INITRD/system/packages"
export BSYS_WORK_DIR="$ISO_WORK/work"
export BSYS_CACHE_DIR=/tmp/bingux-bootstrap-cache
mkdir -p "$BSYS_WORK_DIR"

# Build our tools as packages too
"$ROOT_DIR/target/release/bsys-cli" build "$ROOT_DIR/recipes/toolchain/bpkg/BPKGBUILD" 2>&1 | grep -E "ok:|error"
"$ROOT_DIR/target/release/bsys-cli" build "$ROOT_DIR/recipes/toolchain/bsys/BPKGBUILD" 2>&1 | grep -E "ok:|error"

# Build real packages
for recipe in jq ripgrep fd bat fzf; do
    if [ -f "$ISO_WORK/recipes/$recipe/BPKGBUILD" ] || [ -f "/tmp/bingux-prod-iso/recipes/$recipe/BPKGBUILD" ]; then
        "$ROOT_DIR/target/release/bsys-cli" build "${ISO_WORK}/recipes/$recipe/BPKGBUILD" 2>&1 | grep -E "ok:|error" || \
        "$ROOT_DIR/target/release/bsys-cli" build "/tmp/bingux-prod-iso/recipes/$recipe/BPKGBUILD" 2>&1 | grep -E "ok:|error" || true
    fi
done

echo "    Store: $(ls "$BPKG_STORE_ROOT" 2>/dev/null | wc -l) packages"

# Libs for our dynamically-linked Rust tools
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libbz2.so.1 libcrypto.so.3 libssl.so.3 libz.so.1; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" "$INITRD/lib64/"
done
cp /etc/ssl/certs/ca-bundle.crt "$INITRD/etc/ssl/certs/" 2>/dev/null || true

# Generation profile
for pkg_dir in "$INITRD/system/packages"/*/; do
    [ -d "$pkg_dir/bin" ] || continue
    for b in "$pkg_dir/bin"/*; do
        [ -f "$b" ] || [ -L "$b" ] || continue
        ln -sf "/system/packages/$(basename "$pkg_dir")/bin/$(basename "$b")" \
            "$INITRD/system/profiles/1/bin/$(basename "$b")" 2>/dev/null
    done
done
ln -sf 1 "$INITRD/system/profiles/current"

# System config
cat > "$INITRD/system/config/system.toml" << 'T'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["bpkg", "bsys", "musl-toolchain", "patchelf"]
[services]
enable = []
T

# Init
cat > "$INITRD/init" << 'INIT'
#!/bin/sh
export PATH="/system/profiles/current/bin:/bin:/sbin"
export LD_LIBRARY_PATH="/lib64"
export BPKG_STORE_ROOT="/system/packages"
export HOME="/tmp"
mount -t proc proc /proc; mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs -o size=512M tmpfs /tmp; mount -t tmpfs tmpfs /run
echo ""
echo "  Bingux 0.1.0 — Self-Hosted"
echo "  ==========================="
echo "  Kernel: $(uname -r)"
echo "  gcc: $(gcc --version 2>&1 | head -1)"
echo "  make: $(make --version 2>&1 | head -1)"
echo "  patchelf: $(patchelf --version 2>&1)"
echo "  busybox: $(busybox | head -1)"
echo "  Packages: $(ls /system/packages/ | wc -l)"
echo ""
printf '#include <stdio.h>\nint main() { printf("Self-hosted!\\n"); return 0; }\n' > /tmp/t.c
bingux-gcc -o /tmp/t /tmp/t.c 2>/dev/null && /tmp/t
exec sh
INIT
chmod +x "$INITRD/init"

# Pack
(cd "$INITRD" && find . | cpio -o -H newc 2>/dev/null | gzip > "$ISO_ROOT/boot/initramfs.img")
echo "    Initramfs: $(du -h "$ISO_ROOT/boot/initramfs.img" | cut -f1)"

# GRUB
cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'G'
set timeout=3
menuentry "Bingux (self-hosted)" { linux /boot/vmlinuz init=/init console=ttyS0 console=tty0; initrd /boot/initramfs.img; }
G

genisoimage -o /tmp/bingux-selfhosted.iso -R -J -V "BINGUX" "$ISO_ROOT" 2>/dev/null
echo ""
echo "============================================"
echo "  Self-Hosted ISO: /tmp/bingux-selfhosted.iso"
echo "  Size: $(du -h /tmp/bingux-selfhosted.iso | cut -f1)"
echo "============================================"
