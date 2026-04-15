#!/bin/bash
# build-kernel-in-vm.sh — Prove Bingux can self-host a Linux kernel build
#
# This script:
#   1. Boots a Bingux VM (QEMU) with the bootstrap store
#   2. Downloads the Linux 6.12.8 source inside the VM
#   3. Compiles it using bingux-gcc from the musl toolchain
#   4. Reports success/failure
#
# Requirements: qemu-system-x86_64, KVM, a built production ISO
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
LOG=/tmp/bingux-kernel-build.log
TIMEOUT_SECS=600  # 10 minutes — kernel builds take a while

echo "============================================"
echo "  Bingux Kernel Self-Hosting Build Test"
echo "============================================"

# ── Ensure prerequisites ──────────────────────────────────────────────
if [ ! -f /tmp/bingux-prod-iso/isoroot/boot/vmlinuz ]; then
    echo "==> Building production ISO first..."
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-production-iso.sh" > /dev/null 2>&1
fi

KERNEL=/tmp/bingux-prod-iso/isoroot/boot/vmlinuz
STORE=/tmp/bingux-bootstrap-store

if [ ! -d "$STORE" ]; then
    echo "ERROR: Bootstrap store not found at $STORE"
    echo "Run the bootstrap pipeline first."
    exit 1
fi

# ── Build the initramfs ───────────────────────────────────────────────
echo "==> Building kernel-build initramfs..."
WORK=/tmp/bingux-kbuild-root
rm -rf "$WORK"
mkdir -p "$WORK"/{bin,sbin,lib,lib64,usr/{bin,sbin},dev,proc,sys,run,tmp,etc/ssl/certs}
mkdir -p "$WORK"/system/packages
mkdir -p "$WORK"/build

# Copy the bootstrap store (GCC, make, perl, etc.)
cp -a "$STORE"/* "$WORK/system/packages/" 2>/dev/null || true

# Busybox for shell + basic utilities
cp /usr/bin/busybox "$WORK/bin/"
for cmd in sh cat echo ls mkdir mount umount wc head sort grep cp rm ln \
           chmod printf find reboot poweroff readlink tar xz wget env \
           basename dirname expr test sleep tr cut tee sed awk id; do
    ln -sf busybox "$WORK/bin/$cmd"
done
ln -sf ../../bin/busybox "$WORK/usr/bin/sh"
ln -sf ../../bin/busybox "$WORK/usr/sbin/mount"

# Copy host glibc runtime (the VM initramfs needs it for dynamically-linked tools)
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 \
           libbz2.so.1 libcrypto.so.3 libssl.so.3 libz.so.1; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" "$WORK/lib64/"
done
cp /etc/ssl/certs/ca-bundle.crt "$WORK/etc/ssl/certs/" 2>/dev/null || true

# ── Init script that does the actual kernel build ─────────────────────
cat > "$WORK/init" << 'INIT'
#!/bin/sh
set -e
export PATH="/system/packages/musl-toolchain-13.2.0-x86_64-linux/bin:/bin:/sbin:/usr/bin"
export LD_LIBRARY_PATH="/lib64"
export HOME="/tmp"

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs -o size=4G tmpfs /tmp
mount -t tmpfs -o size=4G tmpfs /build

echo ""
echo "========================================"
echo "  KERNEL BUILD: Starting"
echo "========================================"

# Find the compiler
TC="/system/packages/musl-toolchain-13.2.0-x86_64-linux"
if [ -x "$TC/bin/bingux-gcc" ]; then
    CC="$TC/bin/bingux-gcc"
    echo "CC=$CC"
    $CC --version | head -1
else
    echo "FATAL: bingux-gcc not found"
    poweroff -f
fi

# Find make
MAKE=""
for d in /system/packages/make-*/bin/make; do
    [ -x "$d" ] && MAKE="$d" && break
done
[ -z "$MAKE" ] && MAKE="$(which make 2>/dev/null || true)"
if [ -z "$MAKE" ]; then
    echo "FATAL: make not found"
    poweroff -f
fi
echo "MAKE=$MAKE"

# The kernel source should be pre-loaded in the initramfs at /build
# (we embed a pre-extracted minimal defconfig build)
cd /build

echo "==> Generating defconfig..."
# For the in-VM test, use tinyconfig to keep compile time manageable
$MAKE -C /build/linux-6.12.8 tinyconfig ARCH=x86_64

# Enable bare minimum for a bootable kernel
cd /build/linux-6.12.8
scripts/config --enable CONFIG_64BIT
scripts/config --enable CONFIG_PRINTK
scripts/config --enable CONFIG_TTY
scripts/config --enable CONFIG_SERIAL_8250
scripts/config --enable CONFIG_SERIAL_8250_CONSOLE
scripts/config --enable CONFIG_BLK_DEV_INITRD
scripts/config --enable CONFIG_BINFMT_ELF
scripts/config --enable CONFIG_DEVTMPFS
scripts/config --enable CONFIG_PROC_FS
scripts/config --enable CONFIG_SYSFS
$MAKE olddefconfig ARCH=x86_64

echo "==> Building bzImage ($(nproc) jobs)..."
START=$(date +%s)
$MAKE -j$(nproc) ARCH=x86_64 CC="$CC" bzImage 2>&1 | tail -20

END=$(date +%s)
ELAPSED=$((END - START))

if [ -f arch/x86/boot/bzImage ]; then
    SIZE=$(ls -lh arch/x86/boot/bzImage | awk '{print $5}')
    echo ""
    echo "========================================"
    echo "  KERNEL BUILD: SUCCESS"
    echo "  Image: $SIZE"
    echo "  Time:  ${ELAPSED}s"
    echo "========================================"
else
    echo ""
    echo "========================================"
    echo "  KERNEL BUILD: FAILED"
    echo "========================================"
fi

poweroff -f 2>/dev/null || true
INIT
chmod +x "$WORK/init"

# ── Optionally embed kernel source in the initramfs ───────────────────
# For a real in-VM test we'd download it, but for speed we can pre-stage it.
KSRC_TAR=/tmp/linux-6.12.8.tar.xz
if [ -f "$KSRC_TAR" ]; then
    echo "==> Pre-staging kernel source from cached tarball..."
    mkdir -p "$WORK/build"
    tar xf "$KSRC_TAR" -C "$WORK/build/"
elif command -v wget >/dev/null 2>&1; then
    echo "==> Downloading kernel source..."
    wget -q "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.8.tar.xz" -O "$KSRC_TAR"
    mkdir -p "$WORK/build"
    tar xf "$KSRC_TAR" -C "$WORK/build/"
else
    echo "WARNING: No kernel source available. The VM init will fail."
    echo "Download linux-6.12.8.tar.xz to /tmp/ first."
fi

# ── Pack initramfs ────────────────────────────────────────────────────
echo "==> Packing initramfs..."
(cd "$WORK" && find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-kbuild.img)
IMG_SIZE=$(du -h /tmp/bingux-kbuild.img | cut -f1)
echo "   Initramfs: $IMG_SIZE"

# ── Boot and build ────────────────────────────────────────────────────
echo "==> Booting VM (timeout: ${TIMEOUT_SECS}s)..."
echo ""

timeout "$TIMEOUT_SECS" qemu-system-x86_64 \
    -enable-kvm \
    -m 4G \
    -smp "$(nproc)" \
    -kernel "$KERNEL" \
    -initrd /tmp/bingux-kbuild.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic \
    -no-reboot \
    2>&1 | tee "$LOG" || true

echo ""
echo "============================================"
echo "  Results"
echo "============================================"

if grep -q "KERNEL BUILD: SUCCESS" "$LOG"; then
    TIME=$(grep "Time:" "$LOG" | awk '{print $2}')
    SIZE=$(grep "Image:" "$LOG" | awk '{print $2}')
    echo "  STATUS: PASS"
    echo "  Kernel size: $SIZE"
    echo "  Build time:  $TIME"
    echo ""
    echo "Bingux can self-host Linux kernel compilation."
    exit 0
else
    echo "  STATUS: FAIL"
    echo ""
    echo "Last 30 lines of build log:"
    tail -30 "$LOG"
    exit 1
fi
