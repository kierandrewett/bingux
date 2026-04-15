#!/bin/bash
# Build a Bingux ISO with systemd as init.
# Copies systemd + all deps from the host into the initramfs.
# No root required.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
ISO_WORK=/tmp/bingux-systemd-iso
INITRD="$ISO_WORK/initramfs"
ISO_ROOT="$ISO_WORK/isoroot"
STORE="$ISO_WORK/store"
CACHE="$ISO_WORK/cache"
KERNEL=$(ls /boot/vmlinuz-$(uname -r))

rm -rf "$ISO_WORK"
mkdir -p "$ISO_ROOT/boot/grub" "$STORE" "$CACHE"

echo "============================================"
echo "  Bingux ISO Builder (systemd init)"
echo "============================================"
echo ""

# --- Phase 1: Build tools ---
echo "==> Phase 1: Building Bingux tools"
cargo build --release --manifest-path="$ROOT_DIR/Cargo.toml" 2>&1 | tail -1
echo "    Tools ready"

# --- Phase 2: Build packages ---
echo ""
echo "==> Phase 2: Building packages"
source "$ROOT_DIR/bootstrap/stage2/iso/build-test-iso.sh" --packages-only 2>/dev/null || {
    # If build-test-iso.sh doesn't support --packages-only, build inline
    build_pkg() {
        local name="$1" version="$2" url="$3" script="$4" export="$5"
        local dir="$STORE/${name}-${version}-x86_64-linux"
        [ -d "$dir" ] && return
        echo "    [build] $name $version"
        local work="$ISO_WORK/work/$name/src"
        mkdir -p "$work" "$ISO_WORK/work/$name/pkg"
        local filename="${url##*/}"
        [ -f "$CACHE/$filename" ] || curl -fSL -o "$CACHE/$filename" "$url" 2>&1
        case "$filename" in
            *.tar.gz|*.tgz) tar xzf "$CACHE/$filename" -C "$work" ;;
            *) cp "$CACHE/$filename" "$work/$filename" ;;
        esac
        local pkgdir="$ISO_WORK/work/$name/pkg"
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

    build_pkg "jq" "1.7.1" "https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/jq-linux-amd64" "$PKGDIR/bin/jq"; chmod +x "$PKGDIR/bin/jq"' "bin/jq"
    build_pkg "ripgrep" "14.1.1" "https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/ripgrep-14.1.1-x86_64-unknown-linux-musl/rg" "$PKGDIR/bin/rg"; chmod +x "$PKGDIR/bin/rg"' "bin/rg"
    build_pkg "fd" "10.2.0" "https://github.com/sharkdp/fd/releases/download/v10.2.0/fd-v10.2.0-x86_64-unknown-linux-musl.tar.gz" \
        'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fd-v10.2.0-x86_64-unknown-linux-musl/fd" "$PKGDIR/bin/fd"; chmod +x "$PKGDIR/bin/fd"' "bin/fd"
}

echo "    Packages: $(ls "$STORE" 2>/dev/null | wc -l)"

# --- Phase 3: Build initramfs with systemd ---
echo ""
echo "==> Phase 3: Building initramfs (systemd)"

mkdir -p "$INITRD"/{bin,sbin,lib,lib64,usr/{bin,lib,lib64,sbin,share},dev,proc,sys,run,tmp}
mkdir -p "$INITRD"/etc/{systemd/system,dbus-1/system.d,pam.d,security}
mkdir -p "$INITRD"/system/{packages,profiles/default/bin,config,state}
mkdir -p "$INITRD"/var/{log/journal,run,tmp,lib/systemd}
mkdir -p "$INITRD"/run/{systemd/system,dbus,lock,user}

# Create required empty machine-id (systemd generates on first boot)
touch "$INITRD/etc/machine-id"

# Copy kernel
cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"

# --- Copy systemd + all dependencies ---
echo "    Copying systemd..."

# Helper: copy a binary and all its shared library dependencies
copy_with_deps() {
    local binary="$1"
    [ -f "$binary" ] || return
    local dest="$INITRD/${binary}"
    mkdir -p "$(dirname "$dest")"
    cp -L "$binary" "$dest" 2>/dev/null || true

    # Copy all shared library deps
    ldd "$binary" 2>/dev/null | grep "=> /" | awk '{print $3}' | while read -r lib; do
        local dest="$INITRD/$lib"
        mkdir -p "$(dirname "$dest")"
        [ -f "$dest" ] || cp -L "$lib" "$dest" 2>/dev/null || true
    done
}

# Core systemd binaries
for bin in \
    /usr/lib/systemd/systemd \
    /usr/lib/systemd/systemd-journald \
    /usr/lib/systemd/systemd-logind \
    /usr/bin/systemctl \
    /usr/bin/journalctl \
    /usr/bin/loginctl \
    /usr/lib/systemd/systemd-executor \
    /usr/bin/busctl; do
    if [ -f "$bin" ]; then
        copy_with_deps "$bin"
        echo "      $(basename "$bin")"
    fi
done

# Copy the dynamic linker
cp -L /lib64/ld-linux-x86-64.so.2 "$INITRD/lib64/" 2>/dev/null || true

# Copy systemd shared libraries (they live in a special dir)
if [ -d /usr/lib64/systemd ]; then
    mkdir -p "$INITRD/usr/lib64/systemd"
    cp -L /usr/lib64/systemd/*.so* "$INITRD/usr/lib64/systemd/" 2>/dev/null || true
fi

# Copy ALL essential systemd units + targets
mkdir -p "$INITRD/usr/lib/systemd/system"
# Copy targets (systemd needs these to boot)
for unit in \
    basic.target sysinit.target multi-user.target default.target \
    getty.target graphical.target rescue.target emergency.target \
    shutdown.target reboot.target poweroff.target halt.target \
    local-fs.target local-fs-pre.target remote-fs.target \
    network.target network-pre.target network-online.target \
    nss-lookup.target nss-user-lookup.target \
    paths.target slices.target sockets.target swap.target timers.target \
    umount.target final.target \
    initrd.target initrd-fs.target initrd-root-device.target initrd-root-fs.target \
    tmp.mount \
    getty@.service serial-getty@.service \
    systemd-journald.service systemd-journald.socket systemd-journald-dev-log.socket \
    systemd-tmpfiles-setup.service systemd-tmpfiles-setup-dev-early.service \
    systemd-tmpfiles-clean.service systemd-tmpfiles-clean.timer \
    systemd-sysctl.service \
    systemd-modules-load.service \
    systemd-remount-fs.service \
    systemd-update-utmp.service \
    dbus.service dbus.socket; do
    src="/usr/lib/systemd/system/$unit"
    [ -f "$src" ] && cp "$src" "$INITRD/usr/lib/systemd/system/"
done
# Copy target .wants directories
for wants in /usr/lib/systemd/system/*.target.wants; do
    [ -d "$wants" ] || continue
    dest="$INITRD/$wants"
    mkdir -p "$dest"
    cp -a "$wants"/* "$dest/" 2>/dev/null || true
done
# Copy systemd-tmpfiles binary
copy_with_deps /usr/bin/systemd-tmpfiles 2>/dev/null || true
copy_with_deps /usr/lib/systemd/systemd-modules-load 2>/dev/null || true
copy_with_deps /usr/lib/systemd/systemd-remount-fs 2>/dev/null || true
copy_with_deps /usr/lib/systemd/systemd-sysctl 2>/dev/null || true
copy_with_deps /usr/lib/systemd/systemd-update-utmp 2>/dev/null || true
# Copy tmpfiles.d configs
mkdir -p "$INITRD/usr/lib/tmpfiles.d"
for f in /usr/lib/tmpfiles.d/{systemd,systemd-tmp,tmp,var}.conf; do
    [ -f "$f" ] && cp "$f" "$INITRD/usr/lib/tmpfiles.d/"
done

# Create default.target symlink
ln -sf multi-user.target "$INITRD/usr/lib/systemd/system/default.target" 2>/dev/null || true

# Enable serial console
mkdir -p "$INITRD/usr/lib/systemd/system/getty.target.wants"
ln -sf ../serial-getty@.service "$INITRD/usr/lib/systemd/system/getty.target.wants/serial-getty@ttyS0.service" 2>/dev/null || true

# Busybox for shell + coreutils
cp /usr/bin/busybox "$INITRD/bin/"
for cmd in sh bash cat echo ls mkdir mount umount sleep grep cp rm ln chmod clear reboot poweroff login agetty; do
    ln -sf busybox "$INITRD/bin/$cmd"
done
# Also in /usr/bin and /usr/sbin for systemd unit compatibility
for cmd in sh bash cat echo ls login mount umount; do
    ln -sf ../../bin/busybox "$INITRD/usr/bin/$cmd" 2>/dev/null || true
done
for cmd in mount umount; do
    ln -sf ../../bin/busybox "$INITRD/usr/sbin/$cmd" 2>/dev/null || true
done

# Bingux tools
for tool in bpkg bsys-cli bxc-shim bxc-cli; do
    [ -f "$ROOT_DIR/target/release/$tool" ] && cp "$ROOT_DIR/target/release/$tool" "$INITRD/bin/"
done

# Copy packages
[ -d "$STORE" ] && [ "$(ls -A "$STORE")" ] && cp -a "$STORE"/* "$INITRD/system/packages/"

# Create generation profile
for pkg_dir in "$INITRD/system/packages"/*/; do
    if [ -d "$pkg_dir/bin" ]; then
        for binary in "$pkg_dir/bin"/*; do
            [ -f "$binary" ] || continue
            ln -sf "/system/packages/$(basename "$pkg_dir")/bin/$(basename "$binary")" \
                "$INITRD/system/profiles/default/bin/$(basename "$binary")"
        done
    fi
done

# System config
cat > "$INITRD/system/config/system.toml" << 'TOML'
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["jq", "ripgrep", "fd"]

[services]
enable = []
TOML

# /etc files for systemd
echo "bingux" > "$INITRD/etc/hostname"
echo "LANG=en_GB.UTF-8" > "$INITRD/etc/locale.conf"
echo "KEYMAP=uk" > "$INITRD/etc/vconsole.conf"

# /etc/passwd and /etc/group for login
cat > "$INITRD/etc/passwd" << 'PASSWD'
root:x:0:0:root:/root:/bin/sh
bingux:x:1000:1000:Bingux User:/tmp:/bin/sh
PASSWD

cat > "$INITRD/etc/group" << 'GROUP'
root:x:0:
wheel:x:10:bingux
bingux:x:1000:
GROUP

cat > "$INITRD/etc/shadow" << 'SHADOW'
root::0:0:99999:7:::
bingux::0:0:99999:7:::
SHADOW
chmod 600 "$INITRD/etc/shadow"

# /etc/os-release
cat > "$INITRD/etc/os-release" << 'OSREL'
NAME="Bingux"
ID=bingux
VERSION="0.1.0"
PRETTY_NAME="Bingux 0.1.0"
HOME_URL="https://github.com/kierandrewett/bingux"
OSREL

# Create a bingux welcome service
mkdir -p "$INITRD/usr/lib/systemd/system/multi-user.target.wants"
cat > "$INITRD/usr/lib/systemd/system/bingux-welcome.service" << 'UNIT'
[Unit]
Description=Bingux Welcome Message
After=systemd-journald.service

[Service]
Type=oneshot
ExecStart=/bin/sh -c 'echo ""; echo "  ____  _                       "; echo " | __ )(_)_ __   __ _ _   ___  _"; echo " |  _ \\| | '"'"'_ \\ / _\` | | | \\ \\/ /"; echo " | |_) | | | | | (_| | |_| |>  < "; echo " |____/|_|_| |_|\\__, |\\__,_/_/\\_\\"; echo "                |___/            "; echo ""; echo "  Bingux — powered by systemd"; echo ""; echo "  Packages:"; BPKG_STORE_ROOT=/system/packages PATH=/system/profiles/default/bin:/bin:/usr/bin LD_LIBRARY_PATH=/lib64:/usr/lib64 /bin/bpkg list 2>&1 || echo "  (bpkg unavailable)"; echo ""'
RemainAfterExit=yes
StandardOutput=journal+console
StandardError=journal+console

[Install]
WantedBy=multi-user.target
UNIT
ln -sf ../bingux-welcome.service "$INITRD/usr/lib/systemd/system/multi-user.target.wants/"

# Create an autologin override for serial console
cat > "$INITRD/usr/lib/systemd/system/bingux-shell.service" << 'UNIT'
[Unit]
Description=Bingux Interactive Shell
After=bingux-welcome.service

[Service]
Type=idle
ExecStart=/bin/sh -l
StandardInput=tty
StandardOutput=tty
StandardError=tty
TTYPath=/dev/ttyS0
TTYReset=yes
TTYVHangup=yes
Environment=PATH=/system/profiles/default/bin:/bin:/usr/bin:/sbin
Environment=LD_LIBRARY_PATH=/lib64:/usr/lib64
Environment=BPKG_STORE_ROOT=/system/packages
Environment=HOME=/tmp
Environment=TERM=linux

[Install]
WantedBy=multi-user.target
UNIT
ln -sf ../bingux-shell.service "$INITRD/usr/lib/systemd/system/multi-user.target.wants/"

# Make init point to systemd
ln -sf /usr/lib/systemd/systemd "$INITRD/init"
ln -sf /usr/lib/systemd/systemd "$INITRD/sbin/init"

echo "    initramfs contents: $(find "$INITRD" -type f | wc -l) files"

# Pack initramfs
(cd "$INITRD" && find . | cpio -o -H newc 2>/dev/null | gzip > "$ISO_ROOT/boot/initramfs.img")
echo "    initramfs: $(du -h "$ISO_ROOT/boot/initramfs.img" | cut -f1)"

# --- Phase 4: GRUB config + ISO ---
echo ""
echo "==> Phase 4: Creating ISO"

cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'GRUB'
set timeout=3
set default=0

menuentry "Bingux (systemd)" {
    linux /boot/vmlinuz init=/init console=ttyS0,115200 console=tty0 quiet
    initrd /boot/initramfs.img
}

menuentry "Bingux (systemd verbose)" {
    linux /boot/vmlinuz init=/init console=ttyS0,115200 console=tty0 systemd.log_level=debug
    initrd /boot/initramfs.img
}
GRUB

genisoimage -o /tmp/bingux-systemd.iso -R -J -V "BINGUX" "$ISO_ROOT" 2>/dev/null

echo ""
echo "============================================"
echo "  ISO: /tmp/bingux-systemd.iso ($(du -h /tmp/bingux-systemd.iso | cut -f1))"
echo "============================================"
echo ""
echo "Boot with:"
echo "  qemu-system-x86_64 -enable-kvm -m 1G \\"
echo "    -kernel $ISO_ROOT/boot/vmlinuz \\"
echo "    -initrd $ISO_ROOT/boot/initramfs.img \\"
echo "    -append 'init=/init console=ttyS0 quiet' \\"
echo "    -nographic"
