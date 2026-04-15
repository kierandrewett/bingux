#!/bin/bash
# Build a minimal bootable Bingux test ISO.
# Uses the host kernel + a cpio initramfs with bingux tools.
# No root required.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
ISO_WORK=/tmp/bingux-iso-build
ISO_ROOT="$ISO_WORK/isoroot"
KERNEL=$(ls /boot/vmlinuz-$(uname -r))

rm -rf "$ISO_WORK"
mkdir -p "$ISO_ROOT"/boot/grub

echo "==> Building Bingux Live ISO"
cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"

# Build initramfs
INITRD="$ISO_WORK/initramfs"
mkdir -p "$INITRD"/{bin,lib64,dev,proc,sys,run,tmp}
mkdir -p "$INITRD"/system/{packages/hello-1.0.0-x86_64-linux/{bin,.bpkg},profiles,config,state}

cp /usr/bin/busybox "$INITRD/bin/"
for cmd in sh cat echo ls mkdir mount umount sleep grep cp rm ln chmod clear reboot poweroff; do
    ln -sf busybox "$INITRD/bin/$cmd"
done

cp "$ROOT_DIR/target/release/bpkg" "$INITRD/bin/" 2>/dev/null || echo "WARN: build bpkg first"
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2; do
    find /lib64 -name "$lib" -maxdepth 1 -exec cp {} "$INITRD/lib64/" \; 2>/dev/null || true
done

printf '#!/bin/sh\necho "Hello from Bingux!"\n' > "$INITRD/system/packages/hello-1.0.0-x86_64-linux/bin/hello"
chmod +x "$INITRD/system/packages/hello-1.0.0-x86_64-linux/bin/hello"

cat > "$INITRD/system/packages/hello-1.0.0-x86_64-linux/.bpkg/manifest.toml" << 'T'
[package]
name = "hello"
scope = "bingux"
version = "1.0.0"
arch = "x86_64-linux"
description = "Hello world"
license = "MIT"
[exports]
binaries = ["bin/hello"]
[sandbox]
level = "minimal"
T

cat > "$INITRD/system/config/system.toml" << 'T'
[system]
hostname = "bingux-live"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"
[packages]
keep = ["hello"]
[services]
enable = []
T

cat > "$INITRD/init" << 'INIT'
#!/bin/sh
export PATH="/bin" LD_LIBRARY_PATH="/lib64" BPKG_STORE_ROOT="/system/packages"
mount -t proc proc /proc; mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
mount -t tmpfs tmpfs /tmp; mount -t tmpfs tmpfs /run
echo ""; echo "  Bingux Live Environment"; echo "  ======================"
echo ""; echo "[init] Packages:"; ls /system/packages/
echo ""; echo "[init] hello:"; /system/packages/hello-1.0.0-x86_64-linux/bin/hello
echo ""; echo "[init] bpkg list:"; /bin/bpkg list 2>&1
echo ""; echo "[init] Ready."; exec /bin/sh
INIT
chmod +x "$INITRD/init"

(cd "$INITRD" && find . | cpio -o -H newc 2>/dev/null | gzip > "$ISO_ROOT/boot/initramfs.img")

cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'G'
set timeout=3
menuentry "Bingux Live" { linux /boot/vmlinuz init=/init console=ttyS0 quiet; initrd /boot/initramfs.img; }
G

genisoimage -o /tmp/bingux-live.iso -R -J -V "BINGUX" "$ISO_ROOT" 2>/dev/null
echo "==> /tmp/bingux-live.iso ($(du -h /tmp/bingux-live.iso | cut -f1))"
echo "Test: qemu-system-x86_64 -enable-kvm -m 512M -kernel $ISO_ROOT/boot/vmlinuz -initrd $ISO_ROOT/boot/initramfs.img -append 'init=/init console=ttyS0' -nographic"
