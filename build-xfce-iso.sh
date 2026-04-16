#!/bin/bash
# build-xfce-iso.sh — Build a bootable Bingux XFCE Live ISO from scratch
#
# This single script builds everything needed and produces a GRUB-bootable
# hybrid ISO that boots into a live XFCE desktop on Wayland (labwc).
#
# Requirements:
#   - Fedora (or similar) with: gcc g++ meson ninja pkg-config python3
#     curl cpio gzip genisoimage (or mkisofs) grub2-tools-extra (for grub-mkrescue)
#   - ~15GB free in /tmp
#   - KVM support recommended for testing
#
# Usage:
#   bash build-xfce-iso.sh
#
# Output: /tmp/bingux-xfce-live.iso

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ISO_WORK=/tmp/bingux-iso-work
STORE=/tmp/bingux-bootstrap-store
CACHE=/tmp/bingux-bootstrap-cache
ROOTFS="$ISO_WORK/rootfs"
ISO_ROOT="$ISO_WORK/isoroot"
OUTPUT=/tmp/bingux-xfce-live.iso

mkdir -p "$CACHE" "$ISO_WORK"

log() { echo "==> $*"; }
step() { echo ""; echo "================================================================"; echo "  $*"; echo "================================================================"; }

# ── Check prerequisites ──────────────────────────────────────────────
step "Checking prerequisites"
for cmd in gcc g++ meson ninja pkg-config python3 curl cpio gzip; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "ERROR: $cmd not found. Install it first."
        exit 1
    fi
done

# Check for ISO creation tool
if command -v grub-mkrescue &>/dev/null; then
    ISO_TOOL="grub-mkrescue"
elif command -v grub2-mkrescue &>/dev/null; then
    ISO_TOOL="grub2-mkrescue"
else
    echo "WARNING: grub-mkrescue not found. Install grub2-tools-extra."
    echo "  Fedora: sudo dnf install grub2-tools-extra xorriso"
    echo "  Will fall back to manual ISO creation."
    ISO_TOOL="manual"
fi
log "ISO tool: $ISO_TOOL"

# ── Phase 1: Kernel ──────────────────────────────────────────────────
step "Phase 1: Kernel"
KERNEL="$STORE/linux-kernel-full-6.12.8-x86_64-linux/boot/vmlinuz"
if [ -f "$KERNEL" ]; then
    log "Kernel already built: $KERNEL"
else
    log "Building kernel 6.12.8..."
    KSRC=/tmp/bingux-kernel-full/linux-6.12.8
    if [ ! -d "$KSRC" ]; then
        if [ ! -f "$CACHE/linux-6.12.8.tar.xz" ]; then
            curl -fSL -o "$CACHE/linux-6.12.8.tar.xz" \
                https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.8.tar.xz
        fi
        mkdir -p /tmp/bingux-kernel-full
        tar xf "$CACHE/linux-6.12.8.tar.xz" -C /tmp/bingux-kernel-full
    fi

    # Use host kernel config as base, enable everything needed
    if [ -f "$SCRIPT_DIR/kernel/bingux-kernel-config/config-full" ]; then
        cp "$SCRIPT_DIR/kernel/bingux-kernel-config/config-full" "$KSRC/.config"
    else
        # Use host config
        cp /boot/config-$(uname -r) "$KSRC/.config" 2>/dev/null || make -C "$KSRC" defconfig
    fi
    make -C "$KSRC" olddefconfig
    make -C "$KSRC" -j$(nproc)

    # Install kernel
    KDEST="$STORE/linux-kernel-full-6.12.8-x86_64-linux"
    mkdir -p "$KDEST/boot" "$KDEST/lib/modules/6.12.8"
    cp "$KSRC/arch/x86/boot/bzImage" "$KDEST/boot/vmlinuz"
    cp "$KSRC/.config" "$KDEST/boot/config"
    KERNEL="$KDEST/boot/vmlinuz"
    log "Kernel built: $KERNEL"
fi

# ── Phase 2: XFCE Stack (29 packages) ────────────────────────────────
step "Phase 2: XFCE Desktop Stack"
if [ -d "$STORE/gtk3-src-3.24.43-x86_64-linux" ] && [ -d "$STORE/xfce4-panel-src-4.20.0-x86_64-linux" ]; then
    log "XFCE stack already built"
else
    log "Building XFCE stack (this takes ~15-20 minutes)..."
    bash "$SCRIPT_DIR/bootstrap/stage2/build-xfce-stack.sh"
fi

# ── Phase 3: bingux_compat kernel module ─────────────────────────────
step "Phase 3: bingux_compat kernel module"
COMPAT_MOD="$SCRIPT_DIR/kernel/bingux-compat/bingux_compat.ko"
KSRC=/tmp/bingux-kernel-full/linux-6.12.8
if [ -f "$COMPAT_MOD" ] && [ -f "$KSRC/Module.symvers" ]; then
    log "bingux_compat module already built"
else
    log "Building bingux_compat module..."
    if [ ! -f "$KSRC/Module.symvers" ]; then
        log "Need kernel build tree for module building, using KBUILD_MODPOST_WARN=1"
    fi
    make -C "$SCRIPT_DIR/kernel/bingux-compat" \
        KDIR="$KSRC" KBUILD_MODPOST_WARN=1 2>&1 | tail -5
    log "Module built: $COMPAT_MOD"
fi

# ── Phase 4: Assemble rootfs ─────────────────────────────────────────
step "Phase 4: Assemble live rootfs"
rm -rf "$ROOTFS"
mkdir -p "$ROOTFS"/{io,system/{config,kernel/{proc,sys},modules,state/ephemeral,tmp,profiles/1/{bin,lib,lib64,sbin,usr,share}},users/root/.config/labwc}

PROFILE="$ROOTFS/system/profiles/1"
ln -sf 1 "$ROOTFS/system/profiles/current"

# 4a. Copy busybox (static)
log "Copying busybox..."
if [ -f "$STORE/bash-glibc-5.2.21-x86_64-linux/bin/busybox" ]; then
    cp "$STORE/bash-glibc-5.2.21-x86_64-linux/bin/busybox" "$PROFILE/bin/"
elif command -v busybox &>/dev/null; then
    cp "$(command -v busybox)" "$PROFILE/bin/"
else
    echo "ERROR: busybox not found"
    exit 1
fi

# 4b. Copy host glibc/ld-linux into lib64
log "Copying host glibc..."
cp /lib64/ld-linux-x86-64.so.2 "$PROFILE/lib64/"
cp /lib64/libc.so.6 "$PROFILE/lib64/"

# 4c. Copy XFCE stack libraries (from our builds + host deps)
log "Copying XFCE libraries..."
XFCE_PKGS=(
    glib-src-2.82.4 fribidi-src-1.0.16 harfbuzz-src-10.1.0
    freetype-shared-src-2.13.3 fontconfig-shared-src-2.15.0
    cairo-src-1.18.2 pango-src-1.54.0 gdk-pixbuf-src-2.42.12
    libepoxy-src-1.5.10 graphene-src-1.10.8 at-spi2-core-src-2.54.0
    gtk3-src-3.24.43 libnotify-src-0.8.3 libgudev-src-238
    vte-src-0.74.2 gtk-layer-shell-src-0.9.0
    libxfce4windowing-src-4.20.5 libxfce4util-src-4.20.0
    xfconf-src-4.20.0 libxfce4ui-src-4.20.0 garcon-src-4.20.0
    exo-src-4.20.0 thunar-src-4.20.0 xfce4-panel-src-4.20.0
    xfce4-settings-src-4.20.0 xfce4-terminal-src-1.1.3
    grim-src-1.4.0
)

for pkg in "${XFCE_PKGS[@]}"; do
    pkgdir="$STORE/${pkg}-x86_64-linux"
    [ -d "$pkgdir" ] || continue
    [ -d "$pkgdir/lib" ] && cp -an "$pkgdir/lib/"*.so* "$PROFILE/lib/" 2>/dev/null || true
    [ -d "$pkgdir/bin" ] && cp -an "$pkgdir/bin/"* "$PROFILE/bin/" 2>/dev/null || true
    [ -d "$pkgdir/libexec" ] && { mkdir -p "$PROFILE/libexec"; cp -an "$pkgdir/libexec/"* "$PROFILE/libexec/" 2>/dev/null || true; }
    [ -d "$pkgdir/share" ] && cp -an "$pkgdir/share/"* "$PROFILE/share/" 2>/dev/null || true
    for subdir in gio gdk-pixbuf-2.0 gtk-3.0 xfce4; do
        [ -d "$pkgdir/lib/$subdir" ] && cp -an "$pkgdir/lib/$subdir" "$PROFILE/lib/" 2>/dev/null || true
    done
done

# 4d. Copy xfconfd
if [ -d "$STORE/xfconf-src-4.20.0-x86_64-linux/lib/xfce4" ]; then
    mkdir -p "$PROFILE/lib/xfce4/xfconf"
    cp "$STORE/xfconf-src-4.20.0-x86_64-linux/lib/xfce4/xfconf/xfconfd" "$PROFILE/lib/xfce4/xfconf/"
    ln -sf ../lib/xfce4/xfconf/xfconfd "$PROFILE/bin/xfconfd"
fi

# 4e. Copy labwc, foot, seatd from store
for pkg in labwc-src-0.8.2 labwc-glibc-0.8.2 foot-src-1.19.0 foot-glibc-1.19.0 seatd-src-0.9.1 seatd-glibc-0.9.1; do
    pkgdir="$STORE/${pkg}-x86_64-linux"
    [ -d "$pkgdir" ] || continue
    [ -d "$pkgdir/bin" ] && cp -an "$pkgdir/bin/"* "$PROFILE/bin/" 2>/dev/null || true
    for libdir in "$pkgdir/lib" "$pkgdir/lib64"; do
        [ -d "$libdir" ] && cp -an "$libdir/"*.so* "$PROFILE/lib/" 2>/dev/null || true
    done
done

# 4f. Copy host dbus
log "Copying dbus..."
cp /usr/bin/dbus-daemon "$PROFILE/bin/"
cp /usr/bin/dbus-send "$PROFILE/bin/" 2>/dev/null || true

# 4g. Copy host curl (for networking)
cp /usr/bin/curl "$PROFILE/bin/" 2>/dev/null || true

# 4h. Resolve ALL transitive library dependencies from host
log "Resolving library dependencies..."
for pass in 1 2 3; do
    for target in "$PROFILE"/bin/* "$PROFILE"/lib/*.so*; do
        [ -f "$target" ] || continue
        ldd "$target" 2>/dev/null | awk '{print $3}' | grep '^/' | while read dep; do
            fname=$(basename "$dep")
            [ -f "$PROFILE/lib/$fname" ] && continue
            cp "$dep" "$PROFILE/lib/" 2>/dev/null && true
        done
    done
done
# Remove glibc base libs (must come from lib64, not lib)
rm -f "$PROFILE/lib/libc.so"* "$PROFILE/lib/libm.so"* "$PROFILE/lib/libdl.so"* \
      "$PROFILE/lib/librt.so"* "$PROFILE/lib/libpthread.so"* "$PROFILE/lib/libresolv.so"* \
      "$PROFILE/lib/libcrypt.so"*

log "Libraries: $(ls "$PROFILE/lib/"*.so* 2>/dev/null | wc -l)"

# 4i. Copy host pixbuf loaders
if [ -d /usr/lib64/gdk-pixbuf-2.0 ]; then
    cp -a /usr/lib64/gdk-pixbuf-2.0 "$PROFILE/lib/"
    sed -i "s|/usr/lib64/gdk-pixbuf-2.0|/system/profiles/current/lib/gdk-pixbuf-2.0|g" \
        "$PROFILE/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache" 2>/dev/null || true
fi

# 4j. Copy cursor theme + icons
if [ -d /usr/share/icons/Adwaita/cursors ]; then
    mkdir -p "$PROFILE/share/icons/Adwaita"
    cp -a /usr/share/icons/Adwaita/cursors "$PROFILE/share/icons/Adwaita/"
    cp /usr/share/icons/Adwaita/index.theme "$PROFILE/share/icons/Adwaita/" 2>/dev/null || true
fi

# 4k. Copy XKB data
if [ -d /usr/share/X11/xkb ]; then
    mkdir -p "$PROFILE/usr/share/X11"
    cp -rL /usr/share/X11/xkb "$PROFILE/usr/share/X11/"
fi

# 4l. Copy fonts
if [ -d /usr/share/fonts ]; then
    mkdir -p "$PROFILE/usr/share/fonts"
    cp -a /usr/share/fonts/liberation-sans "$PROFILE/usr/share/fonts/" 2>/dev/null || true
    cp -a /usr/share/fonts/liberation-mono "$PROFILE/usr/share/fonts/" 2>/dev/null || true
    cp -a /usr/share/fonts/google-noto "$PROFILE/usr/share/fonts/" 2>/dev/null || \
    cp -a /usr/share/fonts/dejavu-sans-fonts "$PROFILE/usr/share/fonts/" 2>/dev/null || true
fi

# 4m. Copy SSL certs
mkdir -p "$PROFILE/share/ssl"
cp /etc/pki/tls/certs/ca-bundle.crt "$PROFILE/share/ssl/" 2>/dev/null || \
cp /etc/ssl/certs/ca-certificates.crt "$PROFILE/share/ssl/ca-bundle.crt" 2>/dev/null || true

# 4n. Compile GLib schemas
log "Compiling GLib schemas..."
if [ -d "$PROFILE/share/glib-2.0/schemas" ]; then
    LD_LIBRARY_PATH="$PROFILE/lib" "$PROFILE/bin/glib-compile-schemas" \
        "$PROFILE/share/glib-2.0/schemas/" 2>/dev/null || true
fi

# 4o. Generate MIME database
log "Generating MIME database..."
if command -v update-mime-database &>/dev/null && [ -d "$PROFILE/share/mime" ]; then
    update-mime-database "$PROFILE/share/mime" 2>/dev/null || true
fi

# 4p. Copy bingux_compat module
cp "$SCRIPT_DIR/kernel/bingux-compat/bingux_compat.ko" "$ROOTFS/system/modules/"

# 4q. System config
cat > "$ROOTFS/system/config/system.toml" << 'TOML'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "gb"
TOML

# 4r. labwc autostart
cat > "$ROOTFS/users/root/.config/labwc/autostart" << 'AUTO'
# XFCE Desktop on Bingux
xfce4-panel &
xfce4-terminal &
AUTO

# ── Phase 5: Write init script ───────────────────────────────────────
step "Phase 5: Writing init script"
cat > "$ROOTFS/init" << 'INITEOF'
#!/system/profiles/1/bin/busybox sh
export PATH="/system/profiles/current/bin:/system/profiles/1/bin"
export HOME="/users/root"
export TERM=xterm-256color
export XDG_RUNTIME_DIR="/system/state/ephemeral/xdg"
export WLR_RENDERER=pixman
export WLR_LIBINPUT_NO_DEVICES=1
export LIBSEAT_BACKEND=seatd

LOCALE=$(/system/profiles/1/bin/busybox grep '^locale' /system/config/system.toml 2>/dev/null | /system/profiles/1/bin/busybox sed 's/.*= *"\(.*\)"/\1/')
KEYMAP=$(/system/profiles/1/bin/busybox grep '^keymap' /system/config/system.toml 2>/dev/null | /system/profiles/1/bin/busybox sed 's/.*= *"\(.*\)"/\1/')
export LANG="${LOCALE:-C.UTF-8}"
export LC_ALL="${LOCALE:-C.UTF-8}"
export XKB_DEFAULT_LAYOUT="${KEYMAP:-gb}"

# Mount filesystems
mount -t proc proc /system/kernel/proc
mount -t sysfs sysfs /system/kernel/sys
mount -t devtmpfs devtmpfs /io 2>/dev/null
mount -t tmpfs tmpfs /system/state/ephemeral 2>/dev/null
mount -t tmpfs tmpfs /system/tmp 2>/dev/null

# Bind mounts for /dev /proc /sys (realpath compat)
mkdir -p /dev /proc /sys
mount --bind /io /dev
mount --bind /system/kernel/proc /proc
mount --bind /system/kernel/sys /sys

# Ephemeral dirs
mkdir -p /system/state/ephemeral/etc/fonts /system/state/ephemeral/etc/gtk-3.0
mkdir -p /system/state/ephemeral/run/udev /system/state/ephemeral/run/dbus
mkdir -p /system/state/ephemeral/var /system/state/ephemeral/mnt
mkdir -p /system/state/ephemeral/media /system/state/ephemeral/srv
mkdir -p /system/state/ephemeral/xdg
chmod 0700 /system/state/ephemeral/xdg

# Device setup
mkdir -p /dev/pts /dev/shm
mount -t devpts devpts /dev/pts 2>/dev/null
test -c /dev/tty0 || mknod /dev/tty0 c 4 0 2>/dev/null
test -c /dev/tty1 || mknod /dev/tty1 c 4 1 2>/dev/null
test -c /dev/tty7 || mknod /dev/tty7 c 4 7 2>/dev/null
chmod 666 /dev/tty* 2>/dev/null

# Load bingux_compat
insmod /system/modules/bingux_compat.ko 2>/dev/null

# Generate /etc
printf 'root:x:0:0:root:/users/root:/bin/sh\nnobody:x:65534:65534:nobody:/:/sbin/nologin\n' > /etc/passwd
printf 'root:x:0:\nvideo:x:44:root\ninput:x:97:root\nrender:x:105:root\naudio:x:63:root\nwheel:x:10:root\nnobody:x:65534:\n' > /etc/group
printf 'passwd: files\ngroup: files\nshadow: files\nhosts: files dns\n' > /etc/nsswitch.conf
printf 'nameserver 10.0.2.3\n' > /etc/resolv.conf
cat /proc/sys/kernel/random/uuid | tr -d '-' > /etc/machine-id
mkdir -p /var/lib/dbus && cp /etc/machine-id /var/lib/dbus/machine-id
mkdir -p /system/tmp/fontcache
printf '<?xml version="1.0"?>\n<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">\n<fontconfig><dir>/usr/share/fonts</dir><cachedir>/system/tmp/fontcache</cachedir></fontconfig>\n' > /etc/fonts/fonts.conf
printf '[Settings]\ngtk-theme-name=Adwaita\ngtk-icon-theme-name=hicolor\ngtk-font-name=Sans 10\n' > /etc/gtk-3.0/settings.ini

# Runtime environment
export LD_LIBRARY_PATH="/lib64:/usr/lib64:/system/profiles/current/lib64:/system/profiles/current/lib"
export XDG_DATA_DIRS="/system/profiles/current/share:/usr/share"
export GDK_GL=disable
export GSK_RENDERER=cairo
export GSETTINGS_SCHEMA_DIR="/system/profiles/current/share/glib-2.0/schemas"
export GTK_PATH="/system/profiles/current/lib/gtk-3.0"
export GIO_MODULE_DIR="/system/profiles/current/lib/gio/modules"
export SSL_CERT_FILE="/system/profiles/current/share/ssl/ca-bundle.crt"
export CURL_CA_BUNDLE="/system/profiles/current/share/ssl/ca-bundle.crt"
export DBUS_SESSION_BUS_ADDRESS=""

# Network (QEMU user-mode)
ip link set lo up 2>/dev/null
ip link set eth0 up 2>/dev/null
ip addr add 10.0.2.15/24 dev eth0 2>/dev/null
ip route add default via 10.0.2.2 2>/dev/null

# D-Bus session
cat > /system/state/ephemeral/etc/dbus-session.conf << 'DBUS'
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/run</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
DBUS
dbus-daemon --config-file=/system/state/ephemeral/etc/dbus-session.conf --fork --address="unix:path=/run/dbus-session" 2>/dev/null
export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/dbus-session"

# GPU modules (harmless if built-in)
insmod /system/modules/kernel/virtio_dma_buf.ko 2>/dev/null
insmod /system/modules/kernel/virtio-gpu.ko 2>/dev/null
insmod /system/modules/kernel/virtio_input.ko 2>/dev/null

# udev
systemd-udevd --daemon 2>/dev/null
udevadm trigger 2>/dev/null
udevadm settle --timeout=5 2>/dev/null
sleep 2

# Start Wayland desktop
if ls /dev/dri/card* >/dev/null 2>&1; then
    seatd -l debug -g video 2>/system/tmp/seatd.log &
    sleep 3
    labwc 2>/system/tmp/labwc.log &
    sleep 5
fi

exec sh
INITEOF
chmod +x "$ROOTFS/init"

# ── Phase 6: Create initramfs ────────────────────────────────────────
step "Phase 6: Creating initramfs"
INITRD="$ISO_WORK/initramfs.img"
(cd "$ROOTFS" && find . | cpio -o -H newc 2>/dev/null | gzip) > "$INITRD"
log "Initramfs: $(du -sh "$INITRD" | awk '{print $1}')"

# ── Phase 7: Build ISO ───────────────────────────────────────────────
step "Phase 7: Building bootable ISO"
rm -rf "$ISO_ROOT"
mkdir -p "$ISO_ROOT/boot/grub"

cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"
cp "$INITRD" "$ISO_ROOT/boot/initramfs.img"

cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'GRUB'
set timeout=3
set default=0

menuentry "Bingux v2 — XFCE Desktop (Live)" {
    linux /boot/vmlinuz init=/init quiet
    initrd /boot/initramfs.img
}

menuentry "Bingux v2 — XFCE Desktop (Verbose)" {
    linux /boot/vmlinuz init=/init console=tty0 loglevel=7
    initrd /boot/initramfs.img
}
GRUB

if [ "$ISO_TOOL" = "grub-mkrescue" ] || [ "$ISO_TOOL" = "grub2-mkrescue" ]; then
    log "Building ISO with $ISO_TOOL..."
    "$ISO_TOOL" -o "$OUTPUT" "$ISO_ROOT" 2>&1 | tail -5
else
    log "Building ISO manually with genisoimage..."
    genisoimage -o "$OUTPUT" \
        -b boot/grub/grub.cfg \
        -no-emul-boot \
        -R -J -V "BINGUX_LIVE" \
        "$ISO_ROOT" 2>&1 | tail -5 || {
        # Fallback: just tar it and explain
        log "genisoimage failed, creating tar archive instead"
        tar czf "${OUTPUT%.iso}.tar.gz" -C "$ISO_ROOT" .
        OUTPUT="${OUTPUT%.iso}.tar.gz"
    }
fi

step "BUILD COMPLETE"
echo ""
echo "  ISO: $OUTPUT ($(du -sh "$OUTPUT" | awk '{print $1}'))"
echo ""
echo "  Boot with QEMU:"
echo "    qemu-system-x86_64 -enable-kvm -m 4G -smp 2 \\"
echo "      -cdrom $OUTPUT \\"
echo "      -device virtio-gpu-pci -vga std \\"
echo "      -nic user,model=virtio-net-pci"
echo ""
echo "  Or boot kernel+initrd directly (faster):"
echo "    qemu-system-x86_64 -enable-kvm -m 4G -smp 2 \\"
echo "      -kernel $ISO_ROOT/boot/vmlinuz \\"
echo "      -initrd $ISO_ROOT/boot/initramfs.img \\"
echo "      -append 'init=/init quiet' \\"
echo "      -device virtio-gpu-pci -vga std \\"
echo "      -nic user,model=virtio-net-pci"
echo ""
