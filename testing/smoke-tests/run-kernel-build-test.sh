#!/bin/bash
# Bingux Kernel Self-Hosting Test
# Boots a Bingux VM with the full toolchain and builds Linux 6.12.8 from source.
# This proves full self-hosting: the distro can compile its own kernel.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BOOT_LOG=/tmp/bingux-kernel-build.log
KERNEL_SRC_TAR=/tmp/linux-6.12.8.tar.xz
STORE=/tmp/bingux-bootstrap-store

echo "============================================"
echo "  Bingux Kernel Self-Hosting Build Test"
echo "============================================"
echo ""

# --- Pre-checks ---
if [ ! -f "$KERNEL_SRC_TAR" ]; then
    echo "==> Downloading Linux 6.12.8 source..."
    curl -sL -o "$KERNEL_SRC_TAR" "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.8.tar.xz"
fi
echo "    Kernel source: $(du -h "$KERNEL_SRC_TAR" | cut -f1)"

# Build production ISO if needed (for the host kernel to boot the VM)
if [ ! -f /tmp/bingux-prod-iso/isoroot/boot/vmlinuz ]; then
    echo "==> Building production ISO (for host kernel)..."
    bash "$ROOT_DIR/bootstrap/stage2/iso/build-production-iso.sh" > /dev/null 2>&1
fi
VM_KERNEL=/tmp/bingux-prod-iso/isoroot/boot/vmlinuz

# --- Build initramfs ---
echo "==> Building kernel-build initramfs..."
FRESH=/tmp/bingux-kernel-build-initramfs
rm -rf "$FRESH"
mkdir -p "$FRESH"/{bin,sbin,lib,lib64,usr/{bin,sbin,lib},dev,proc,sys,run,tmp,etc/ssl/certs}
mkdir -p "$FRESH"/system/{packages,profiles/1/bin,config,state,tmp}

# Busybox (provides sh, cat, echo, ls, cp, ln, chmod, mkdir, etc.)
cp /usr/bin/busybox "$FRESH/bin/"
for cmd in $(/usr/bin/busybox --list 2>/dev/null); do
    ln -sf busybox "$FRESH/bin/$cmd" 2>/dev/null || true
done
ln -sf ../../bin/busybox "$FRESH/usr/bin/sh" 2>/dev/null
ln -sf ../../bin/busybox "$FRESH/usr/sbin/mount" 2>/dev/null

# musl GCC toolchain (self-contained: gcc, ld, as, ar, make, headers, libc)
# This is the primary compiler for the kernel build - all statically linked
echo "    Copying musl toolchain (GCC + binutils + headers)..."
cp -a "$STORE/musl-toolchain-13.2.0-x86_64-linux" "$FRESH/system/packages/"

# Binutils from source (ar, as, nm, objcopy, objdump, ranlib, readelf, strip)
# These supplement the musl toolchain with newer versions
echo "    Copying binutils..."
cp -a "$STORE/binutils-src-2.43.1-x86_64-linux" "$FRESH/system/packages/"

# GCC 14.2.0 from source (statically linked) - used as CC for the kernel
# Has cc1/collect2 in libexec but needs headers from the musl toolchain
echo "    Copying GCC 14.2.0..."
cp -a "$STORE/gcc-src-14.2.0-x86_64-linux" "$FRESH/system/packages/"

# Perl (needed by kernel build scripts)
echo "    Copying perl..."
cp -a "$STORE/perl-src-5.40.0-x86_64-linux" "$FRESH/system/packages/"

# Flex + bison (needed for kernel config processing)
echo "    Copying flex/bison..."
cp -a "$STORE/flex-src-2.6.4-x86_64-linux" "$FRESH/system/packages/"
cp -a "$STORE/bison-src-3.8.2-x86_64-linux" "$FRESH/system/packages/"

# bc (needed by kernel build for timeconst)
echo "    Copying bc..."
cp -a "$STORE/bc-src-1.07.1-x86_64-linux" "$FRESH/system/packages/"

# Core utilities needed by kernel Makefile
for pkg in sed-src-4.9-x86_64-linux grep-src-3.11-x86_64-linux gawk-src-5.3.1-x86_64-linux \
    findutils-src-4.10.0-x86_64-linux diffutils-src-3.10-x86_64-linux tar-src-1.35-x86_64-linux \
    gzip-src-1.13-x86_64-linux xz-src-5.6.3-x86_64-linux coreutils-src-9.5-x86_64-linux; do
    if [ -d "$STORE/$pkg" ]; then
        echo "    Copying $pkg..."
        cp -a "$STORE/$pkg" "$FRESH/system/packages/"
    fi
done

# Pre-stage kernel source tarball
echo "    Staging kernel source tarball..."
cp "$KERNEL_SRC_TAR" "$FRESH/system/tmp/linux-src.tar.xz"

# Create generation profile: symlink all package bins
for pkg_dir in "$FRESH/system/packages"/*/; do
    [ -d "$pkg_dir/bin" ] || continue
    for b in "$pkg_dir/bin"/*; do
        [ -f "$b" ] || [ -L "$b" ] || continue
        ln -sf "/system/packages/$(basename "$pkg_dir")/bin/$(basename "$b")" \
            "$FRESH/system/profiles/1/bin/$(basename "$b")" 2>/dev/null || true
    done
done
ln -sf 1 "$FRESH/system/profiles/current"

# Libs for any dynamically linked binaries (perl etc.)
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libbz2.so.1 \
    libcrypto.so.3 libssl.so.3 libz.so.1 libpthread.so.0 libdl.so.2 libcrypt.so.1 \
    libcrypt.so.2 libresolv.so.2; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" "$FRESH/lib64/" 2>/dev/null || true
done
cp /etc/ssl/certs/ca-bundle.crt "$FRESH/etc/ssl/certs/" 2>/dev/null || true

# Init script: extract kernel, configure, build
cat > "$FRESH/init" << 'INIT'
#!/bin/sh
# Bingux Kernel Build Self-Hosting Test
#
# Uses bingux-gcc (musl GCC wrapper) as both CC and HOSTCC.
# The musl toolchain is self-contained: gcc + headers + libc + ld + as.
# The kernel builds host tools (fixdep, etc.) with HOSTCC and the kernel
# itself with CC. Both use the musl-based GCC with full headers.

# Set up the musl toolchain as primary - it has headers for host compilation
MUSL_ROOT="/system/packages/musl-toolchain-13.2.0-x86_64-linux"
export PATH="/system/profiles/current/bin:$MUSL_ROOT/bin:/bin:/sbin:/usr/bin"
export LD_LIBRARY_PATH="/lib64"
export HOME="/tmp"
# Tell GCC where to find its headers and libs
export C_INCLUDE_PATH="$MUSL_ROOT/include"
export LIBRARY_PATH="$MUSL_ROOT/lib"

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true

# CRITICAL: 3GB tmpfs for kernel build workspace (source + build artifacts)
mount -t tmpfs -o size=3G tmpfs /tmp
mount -t tmpfs tmpfs /run

echo ""
echo "============================================"
echo "  Bingux Kernel Build Self-Hosting Test"
echo "============================================"
echo ""

# Verify tools
echo "==> [1/6] Verifying toolchain..."
echo "  Kernel:     $(uname -r)"
echo "  bingux-gcc: $(bingux-gcc --version 2>&1 | head -1)"
echo "  gcc:        $(gcc --version 2>&1 | head -1)"
echo "  make:       $(make --version 2>&1 | head -1)"
echo "  perl:       $(perl --version 2>&1 | grep version | head -1)"
echo "  flex:       $(flex --version 2>&1 | head -1)"
echo "  bison:      $(bison --version 2>&1 | head -1)"
echo "  bc:         $(echo '1+1' | bc 2>&1)"
echo "  as:         $(as --version 2>&1 | head -1)"
echo "  ld:         $(ld --version 2>&1 | head -1)"
echo "  ar:         $(ar --version 2>&1 | head -1)"
echo ""

# Quick compiler sanity check
echo "  Compiler sanity check..."
printf '#include <stdio.h>\nint main(){puts("OK");return 0;}\n' > /tmp/test.c
if bingux-gcc -static -o /tmp/test /tmp/test.c 2>&1 && [ "$(/tmp/test)" = "OK" ]; then
    echo "  bingux-gcc: compiles and runs OK (static musl)"
else
    echo "  WARNING: bingux-gcc sanity check failed, trying plain gcc..."
    gcc -isystem "$MUSL_ROOT/include" -L"$MUSL_ROOT/lib" -static -o /tmp/test /tmp/test.c 2>&1
    /tmp/test 2>&1
fi
rm -f /tmp/test /tmp/test.c
echo ""

# Extract kernel source
echo "==> [2/6] Extracting kernel source..."
BUILD_START=$(date +%s)
cd /tmp
tar xf /system/tmp/linux-src.tar.xz
echo "  Extracted to /tmp/linux-6.12.8"
echo "  Source size: $(du -sh /tmp/linux-6.12.8 | cut -f1)"
echo ""

cd /tmp/linux-6.12.8

# Configure: tinyconfig + essential QEMU boot options
# Use bingux-gcc which has proper include paths via the wrapper
echo "==> [3/6] Configuring kernel (tinyconfig + QEMU essentials)..."
# HOSTLDFLAGS=-static ensures host tools (fixdep, etc.) are statically linked
# against musl, since there's no dynamic linker in the initramfs
make tinyconfig CC=bingux-gcc HOSTCC=bingux-gcc HOSTLDFLAGS=-static 2>&1 | tail -5
echo ""

# Enable essential configs for a bootable QEMU kernel
echo "==> [4/6] Enabling essential boot configs..."
./scripts/config --enable CONFIG_64BIT
./scripts/config --enable CONFIG_PRINTK
./scripts/config --enable CONFIG_SERIAL_8250
./scripts/config --enable CONFIG_SERIAL_8250_CONSOLE
./scripts/config --enable CONFIG_TTY
./scripts/config --enable CONFIG_BINFMT_ELF
./scripts/config --enable CONFIG_BLK_DEV_INITRD
./scripts/config --enable CONFIG_HAS_IOMEM
./scripts/config --enable CONFIG_HAS_IOPORT
./scripts/config --enable CONFIG_BLOCK
echo "  Configs set."

# Update .config to resolve dependencies
make olddefconfig CC=bingux-gcc HOSTCC=bingux-gcc HOSTLDFLAGS=-static 2>&1 | tail -3
echo ""
echo "  Config options:"
grep -c '=y' .config | xargs printf "    %s options enabled\n"
echo ""

# Build
NCPUS=$(nproc 2>/dev/null || echo 4)
echo "==> [5/6] Building kernel (make -j$NCPUS bzImage)..."
echo "  Build started at: $(date)"
MAKE_START=$(date +%s)

make -j"$NCPUS" CC=bingux-gcc HOSTCC=bingux-gcc HOSTLDFLAGS=-static bzImage 2>&1 | tail -30

MAKE_END=$(date +%s)
MAKE_DURATION=$((MAKE_END - MAKE_START))
echo ""
echo "  Build finished at: $(date)"
echo "  Build duration: ${MAKE_DURATION}s"
echo ""

# Verify
echo "==> [6/6] Verifying build output..."
if [ -f arch/x86/boot/bzImage ]; then
    BUILD_END=$(date +%s)
    TOTAL_DURATION=$((BUILD_END - BUILD_START))
    BZIMAGE_SIZE=$(du -h arch/x86/boot/bzImage | cut -f1)
    echo "  KERNEL BUILD SUCCESS"
    echo "  bzImage: arch/x86/boot/bzImage ($BZIMAGE_SIZE)"
    echo "  Total time: ${TOTAL_DURATION}s (extract + config + build)"
    echo "  Build time: ${MAKE_DURATION}s (make only)"
    echo ""
    echo "  File info:"
    file arch/x86/boot/bzImage 2>/dev/null || ls -la arch/x86/boot/bzImage
    echo ""
    echo "  Disk usage:"
    echo "    Build dir: $(du -sh /tmp/linux-6.12.8 | cut -f1)"
    echo "    tmpfs:     $(df -h /tmp | tail -1 | awk '{print $3 " used / " $2 " total"}')"
    echo ""
    echo "PASS: Bingux can build the Linux kernel from source"
else
    echo "  KERNEL BUILD FAILED"
    echo "  bzImage not found at arch/x86/boot/bzImage"
    echo "  Checking for partial output..."
    ls -la arch/x86/boot/ 2>/dev/null || echo "  No boot dir"
    echo ""
    echo "  Last 50 lines of build:"
    echo "  (check log for details)"
    echo ""
    echo "  Disk usage:"
    df -h /tmp | tail -1
    echo ""
    echo "FAIL: kernel build did not produce bzImage"
fi

echo ""
echo "=== KERNEL BUILD TEST COMPLETE ==="
poweroff -f 2>/dev/null || true
INIT
chmod +x "$FRESH/init"

# Pack initramfs
echo "==> Packing initramfs..."
(cd "$FRESH" && find . | cpio -o -H newc 2>/dev/null | gzip > /tmp/bingux-kernel-build.img)
echo "    Initramfs: $(du -h /tmp/bingux-kernel-build.img | cut -f1)"
echo ""

# Detect KVM
ACCEL_ARGS=""
if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    ACCEL_ARGS="-enable-kvm -cpu host"
    echo "    Using KVM acceleration"
else
    ACCEL_ARGS="-cpu max"
    echo "    WARN: No KVM, using software emulation (will be slow)"
fi

# Boot VM with generous timeout (kernel build can take minutes)
echo "==> Booting VM (600s timeout for kernel build)..."
echo "    VM: 6GB RAM, 4 CPUs"
echo ""
timeout 600 qemu-system-x86_64 \
    $ACCEL_ARGS -m 6G -smp 4 \
    -kernel "$VM_KERNEL" -initrd /tmp/bingux-kernel-build.img \
    -append "init=/init console=ttyS0 quiet" \
    -nographic -no-reboot \
    2>&1 | tee "$BOOT_LOG" || true

echo ""
echo "============================================"
echo "  Results"
echo "============================================"
PASS=0; FAIL=0
check() {
    if grep -q "$1" "$BOOT_LOG"; then
        echo "  PASS: $2"
        PASS=$((PASS+1))
    else
        echo "  FAIL: $2"
        FAIL=$((FAIL+1))
    fi
}

check "Kernel Build Self-Hosting" "Test started"
check "gcc.*GCC" "GCC available"
check "Extracting kernel" "Kernel source extracted"
check "tinyconfig" "tinyconfig applied"
check "Enabling essential" "Boot configs enabled"
check "Building kernel" "Build started"
check "KERNEL BUILD SUCCESS" "bzImage produced"
check "PASS.*build the Linux kernel" "Self-hosting verified"
check "KERNEL BUILD TEST COMPLETE" "Test completed"

echo ""
echo "Results: $PASS passed, $FAIL failed"
echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "ALL TESTS PASSED — Bingux can self-host Linux kernel builds"
else
    echo "SOME TESTS FAILED"
    echo "Full log: $BOOT_LOG"
fi
echo ""
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
