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

# ── Build bsys-cli (needed for all phases) ──────────────────────────
BSYS="$SCRIPT_DIR/target/release/bsys-cli"
if [ ! -x "$BSYS" ]; then
    step "Building bsys-cli"
    cargo build --release --manifest-path="$SCRIPT_DIR/Cargo.toml" --bin bsys-cli 2>&1 | tail -3
fi

export BPKG_STORE_ROOT="$STORE"
export BSYS_CACHE_DIR="$CACHE"

# Helper: build a package via bsys, report status
bsys_build() {
    local recipe="$1" name="$2"
    local result
    result=$("$BSYS" build "$recipe" 2>&1)
    if echo "$result" | grep -q "ok: built"; then
        log "  Built: $name"
        return 0
    elif echo "$result" | grep -q "already exists"; then
        log "  Cached: $name"
        return 0
    else
        echo "  FAILED: $name"
        echo "$result" | tail -5
        return 1
    fi
}

# ── Phase 1: Kernel ──────────────────────────────────────────────────
step "Phase 1: Kernel"
KERNEL="$STORE/linux-kernel-full-6.14.6-x86_64-linux/boot/vmlinuz"
bsys_build "$SCRIPT_DIR/recipes/core/linux/BPKGBUILD" "linux-kernel-full-6.14.6" || {
    echo "ERROR: Kernel build failed"; exit 1
}

# ── Phase 2: bingux_compat kernel module ─────────────────────────────
step "Phase 2: bingux_compat kernel module"
bsys_build "$SCRIPT_DIR/recipes/core/bingux-compat/BPKGBUILD" "bingux-compat-1.1" || {
    echo "ERROR: bingux_compat module build failed"; exit 1
}

# ── Phase 3: XFCE Stack (25 packages via bsys) ──────────────────────
step "Phase 3: XFCE Desktop Stack"
log "Building XFCE stack via bsys..."

DESKTOP_PKGS=(
    glib-src fribidi-src harfbuzz-src cairo-src pango-src
    gdk-pixbuf-src libepoxy-src graphene-src at-spi2-core-src
    gtk3-src libnotify-src libgudev-src vte-src gtk-layer-shell-src
    libxfce4util-src xfconf-src libxfce4ui-src libxfce4windowing-src
    garcon-src exo-src thunar-src xfce4-panel-src
    xfce4-settings-src xfce4-terminal-src grim-src
)

FAILED=0
for pkg in "${DESKTOP_PKGS[@]}"; do
    bsys_build "$SCRIPT_DIR/recipes/desktop/$pkg/BPKGBUILD" "$pkg" || FAILED=$((FAILED+1))
done
if [ "$FAILED" -gt 0 ]; then
    echo "ERROR: $FAILED packages failed to build."
    exit 1
fi

# ── Phase 4: Assemble rootfs ─────────────────────────────────────────
step "Phase 4: Assemble live rootfs"
rm -rf "$ROOTFS"
mkdir -p "$ROOTFS"/{io,system/{config,kernel/{proc,sys},packages,state/ephemeral,tmp,profiles},users/root/.config/labwc}

# 4a. Copy packages into rootfs store
log "Copying packages to rootfs store..."
ALL_PKGS=(
    # Kernel + module
    linux-kernel-full-6.14.6
    bingux-compat-1.1
    # Desktop stack
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
    # Wayland/compositor stack (from bootstrap)
    wayland-src-1.23.1 wayland-protocols-src-1.38
    wlroots-src-0.18.2 labwc-src-0.8.2 seatd-src-0.9.1
    foot-src-1.19.0 fcft-src-3.1.9
    xkbcommon-src-1.7.0 libdrm-src-2.4.123
    pixman-src-0.43.4 mesa-src-24.3.4
    libinput-src-1.27.0 libevdev-src-1.13.3 mtdev-src-1.1.7
    libdisplay-info-src-0.2.0
    # Core
    dbus-src-1.14.10 systemd-src-256.11
    alsa-lib-src-1.2.12 pipewire-src-1.2.7
    libffi-src-3.4.6 pcre2-src-10.44
    zlib-1.3.1 expat-src-2.6.4 libpng-src-1.6.44
    bzip2-src-1.0.8 openssl-src-3.4.1
    libcap-src-2.70 libunistring-src-1.3
    shared-mime-info-src-2.4 iso-codes-src-4.17.0
    hicolor-icon-theme-src-0.17
)

for pkg in "${ALL_PKGS[@]}"; do
    pkgdir="$STORE/${pkg}-x86_64-linux"
    if [ -d "$pkgdir" ]; then
        cp -a "$pkgdir" "$ROOTFS/system/packages/"
    fi
done
log "Copied $(ls -d "$ROOTFS/system/packages/"*/ 2>/dev/null | wc -l) packages"

# 4b. Write system.toml with package list
log "Writing system.toml..."
KEEP_LIST=""
for pkg in "${ALL_PKGS[@]}"; do
    KEEP_LIST="${KEEP_LIST}  \"${pkg}\",\n"
done

cat > "$ROOTFS/system/config/system.toml" << TOML
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "gb"

[packages]
keep = [
$(printf "$KEEP_LIST")
]

[services]
enable = []
TOML

# 4c. Compose system profile via bsys apply
log "Composing system profile..."
BSYS_CONFIG_PATH="$ROOTFS/system/config/system.toml" \
BSYS_PROFILES_ROOT="$ROOTFS/system/profiles" \
BSYS_PACKAGES_ROOT="$ROOTFS/system/packages" \
BPKG_STORE_ROOT="$ROOTFS/system/packages" \
"$BSYS" apply 2>&1 | grep -E '\[bsys\]'

PROFILE="$ROOTFS/system/profiles/current"
if [ ! -d "$PROFILE" ] && [ ! -L "$PROFILE" ]; then
    echo "ERROR: bsys apply failed to create profile"
    exit 1
fi
log "Profile created at $PROFILE"

# 4d. Post-profile: add host runtime data not yet packaged
# TODO: these should become proper bsys packages (fonts, cursors, xkb, ssl-certs)
PROFILE_DIR=$(readlink -f "$ROOTFS/system/profiles/current" 2>/dev/null || echo "$ROOTFS/system/profiles/1")
log "Adding host runtime data to profile..."

# Host glibc (needed until all binaries are patchelf'd to store glibc)
mkdir -p "$PROFILE_DIR/lib64"
cp /lib64/ld-linux-x86-64.so.2 "$PROFILE_DIR/lib64/" 2>/dev/null || true
cp /lib64/libc.so.6 "$PROFILE_DIR/lib64/" 2>/dev/null || true

# XKB keyboard data
if [ -d /usr/share/X11/xkb ]; then
    mkdir -p "$PROFILE_DIR/usr/share/X11"
    cp -rL /usr/share/X11/xkb "$PROFILE_DIR/usr/share/X11/"
fi

# Fonts
mkdir -p "$PROFILE_DIR/usr/share/fonts"
cp -a /usr/share/fonts/liberation-sans "$PROFILE_DIR/usr/share/fonts/" 2>/dev/null || true
cp -a /usr/share/fonts/liberation-mono "$PROFILE_DIR/usr/share/fonts/" 2>/dev/null || true

# Cursor theme
if [ -d /usr/share/icons/Adwaita/cursors ]; then
    mkdir -p "$PROFILE_DIR/share/icons/Adwaita"
    cp -a /usr/share/icons/Adwaita/cursors "$PROFILE_DIR/share/icons/Adwaita/"
    cp /usr/share/icons/Adwaita/index.theme "$PROFILE_DIR/share/icons/Adwaita/" 2>/dev/null || true
fi

# SSL certificates
mkdir -p "$PROFILE_DIR/share/ssl"
cp /etc/pki/tls/certs/ca-bundle.crt "$PROFILE_DIR/share/ssl/" 2>/dev/null || \
cp /etc/ssl/certs/ca-certificates.crt "$PROFILE_DIR/share/ssl/ca-bundle.crt" 2>/dev/null || true

# 4e. Compile GLib schemas (profile needs compiled schemas)
log "Compiling GLib schemas..."
SCHEMA_DIR="$PROFILE_DIR/share/glib-2.0/schemas"
if [ -d "$SCHEMA_DIR" ]; then
    glib-compile-schemas "$SCHEMA_DIR" 2>/dev/null || true
fi

# 4f. Generate MIME database
log "Generating MIME database..."
if command -v update-mime-database &>/dev/null && [ -d "$PROFILE_DIR/share/mime" ]; then
    update-mime-database "$PROFILE_DIR/share/mime" 2>/dev/null || true
fi

# 4g. labwc autostart
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
insmod /system/profiles/current/lib/modules/bingux_compat.ko 2>/dev/null

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
# GPU modules (harmless failures if built-in to kernel)
insmod /system/profiles/current/lib/modules/virtio_dma_buf.ko 2>/dev/null
insmod /system/profiles/current/lib/modules/virtio-gpu.ko 2>/dev/null
insmod /system/profiles/current/lib/modules/virtio_input.ko 2>/dev/null

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

if command -v xorriso &>/dev/null && { [ "$ISO_TOOL" = "grub-mkrescue" ] || [ "$ISO_TOOL" = "grub2-mkrescue" ]; }; then
    log "Building ISO with $ISO_TOOL..."
    "$ISO_TOOL" -o "$OUTPUT" "$ISO_ROOT" 2>&1 | tail -5
else
    log "xorriso not available — skipping ISO creation"
    log "You can boot directly with kernel+initrd (faster anyway):"
    OUTPUT="(no ISO — use kernel+initrd below)"
fi

step "BUILD COMPLETE"
echo ""
if [ -f "$OUTPUT" ]; then
    echo "  ISO: $OUTPUT ($(du -sh "$OUTPUT" | awk '{print $1}'))"
    echo ""
    echo "  Boot with QEMU:"
    echo "    qemu-system-x86_64 -enable-kvm -m 4G -smp 2 \\"
    echo "      -cdrom $OUTPUT \\"
    echo "      -device virtio-gpu-pci -vga std \\"
    echo "      -nic user,model=virtio-net-pci"
fi
echo ""
echo "  Boot kernel+initrd directly (recommended):"
echo "    qemu-system-x86_64 -enable-kvm -m 4G -smp 2 \\"
echo "      -kernel $ISO_ROOT/boot/vmlinuz \\"
echo "      -initrd $ISO_ROOT/boot/initramfs.img \\"
echo "      -append 'init=/init quiet' \\"
echo "      -device virtio-gpu-pci -vga std \\"
echo "      -nic user,model=virtio-net-pci"
echo ""
