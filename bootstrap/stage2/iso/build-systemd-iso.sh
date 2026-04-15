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
build_pkg() {
        local name="$1" version="$2" url="$3" script="$4" export="$5"
        local dir="$STORE/${name}-${version}-x86_64-linux"
        [ -d "$dir" ] && return
        echo "    [build] $name $version"
        local work="$ISO_WORK/work/$name/src"
        mkdir -p "$work" "$ISO_WORK/work/$name/pkg"
        local filename="${url##*/}"
        if [ ! -f "$CACHE/$filename" ]; then
            if ! curl -fSL -o "$CACHE/$filename" "$url" 2>&1; then
                echo "      WARN: download failed, skipping $name"
                rm -f "$CACHE/$filename"
                return
            fi
        fi
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
build_pkg "bat" "0.24.0" "https://github.com/sharkdp/bat/releases/download/v0.24.0/bat-v0.24.0-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/bat-v0.24.0-x86_64-unknown-linux-musl/bat" "$PKGDIR/bin/bat"; chmod +x "$PKGDIR/bin/bat"' "bin/bat"
build_pkg "eza" "0.20.14" "https://github.com/eza-community/eza/releases/download/v0.20.14/eza_x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/eza" "$PKGDIR/bin/eza"; chmod +x "$PKGDIR/bin/eza"' "bin/eza"
build_pkg "delta" "0.18.2" "https://github.com/dandavison/delta/releases/download/0.18.2/delta-0.18.2-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/delta-0.18.2-x86_64-unknown-linux-musl/delta" "$PKGDIR/bin/delta"; chmod +x "$PKGDIR/bin/delta"' "bin/delta"
build_pkg "zoxide" "0.9.6" "https://github.com/ajeetdsouza/zoxide/releases/download/v0.9.6/zoxide-0.9.6-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/zoxide" "$PKGDIR/bin/zoxide"; chmod +x "$PKGDIR/bin/zoxide"' "bin/zoxide"
build_pkg "fzf" "0.57.0" "https://github.com/junegunn/fzf/releases/download/v0.57.0/fzf-0.57.0-linux_amd64.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fzf" "$PKGDIR/bin/fzf"; chmod +x "$PKGDIR/bin/fzf"' "bin/fzf"
build_pkg "dust" "1.1.1" "https://github.com/bootandy/dust/releases/download/v1.1.1/dust-v1.1.1-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/dust-v1.1.1-x86_64-unknown-linux-musl/dust" "$PKGDIR/bin/dust"; chmod +x "$PKGDIR/bin/dust"' "bin/dust"

# Neovim (self-contained release)
build_pkg "neovim" "0.10.4" "https://github.com/neovim/neovim/releases/download/v0.10.4/nvim-linux-x86_64.tar.gz" \
    'mkdir -p "$PKGDIR/bin" "$PKGDIR/lib" "$PKGDIR/share"; cp "$SRCDIR/nvim-linux-x86_64/bin/nvim" "$PKGDIR/bin/nvim"; chmod +x "$PKGDIR/bin/nvim"; cp -a "$SRCDIR/nvim-linux-x86_64/lib/"* "$PKGDIR/lib/" 2>/dev/null || true; cp -a "$SRCDIR/nvim-linux-x86_64/share/"* "$PKGDIR/share/" 2>/dev/null || true' "bin/nvim"

# curl (static binary)
build_pkg "curl" "8.10.1" "https://github.com/moparisthebest/static-curl/releases/download/v8.10.1/curl-amd64" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/curl-amd64" "$PKGDIR/bin/curl"; chmod +x "$PKGDIR/bin/curl"' "bin/curl"

# Python (standalone build from indygreg)
build_pkg "python" "3.12.9" "https://github.com/indygreg/python-build-standalone/releases/download/20250317/cpython-3.12.9+20250317-x86_64-unknown-linux-musl-install_only_stripped.tar.gz" \
    'mkdir -p "$PKGDIR/bin" "$PKGDIR/lib"; cp -a "$SRCDIR/python/bin/python3.12" "$PKGDIR/bin/python3"; ln -sf python3 "$PKGDIR/bin/python"; cp -a "$SRCDIR/python/lib/python3.12" "$PKGDIR/lib/" 2>/dev/null || true; chmod +x "$PKGDIR/bin/python3"' "bin/python3"

# Node.js
build_pkg "nodejs" "22.14.0" "https://nodejs.org/dist/v22.14.0/node-v22.14.0-linux-x64.tar.gz" \
    'mkdir -p "$PKGDIR/bin" "$PKGDIR/lib"; cp "$SRCDIR/node-v22.14.0-linux-x64/bin/node" "$PKGDIR/bin/node"; chmod +x "$PKGDIR/bin/node"' "bin/node"

# ---- 10 more essential packages ----

# starship (cross-shell prompt)
build_pkg "starship" "1.22.1" "https://github.com/starship/starship/releases/download/v1.22.1/starship-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/starship" "$PKGDIR/bin/starship"; chmod +x "$PKGDIR/bin/starship"' "bin/starship"

# lazygit (terminal git UI)
build_pkg "lazygit" "0.44.1" "https://github.com/jesseduffield/lazygit/releases/download/v0.44.1/lazygit_0.44.1_Linux_x86_64.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/lazygit" "$PKGDIR/bin/lazygit"; chmod +x "$PKGDIR/bin/lazygit"' "bin/lazygit"

# bottom (system monitor)
build_pkg "bottom" "0.10.2" "https://github.com/ClementTsang/bottom/releases/download/0.10.2/bottom_x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/btm" "$PKGDIR/bin/btm"; chmod +x "$PKGDIR/bin/btm"' "bin/btm"

# yq (YAML processor)
build_pkg "yq" "4.45.1" "https://github.com/mikefarah/yq/releases/download/v4.45.1/yq_linux_amd64.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; find "$SRCDIR" -name "yq*" -type f | head -1 | xargs -I{} cp {} "$PKGDIR/bin/yq"; chmod +x "$PKGDIR/bin/yq"' "bin/yq"

# hexyl (hex viewer)
build_pkg "hexyl" "0.14.0" "https://github.com/sharkdp/hexyl/releases/download/v0.14.0/hexyl-v0.14.0-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/hexyl-v0.14.0-x86_64-unknown-linux-musl/hexyl" "$PKGDIR/bin/hexyl"; chmod +x "$PKGDIR/bin/hexyl"' "bin/hexyl"

# hyperfine (benchmarking)
build_pkg "hyperfine" "1.19.0" "https://github.com/sharkdp/hyperfine/releases/download/v1.19.0/hyperfine-v1.19.0-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/hyperfine-v1.19.0-x86_64-unknown-linux-musl/hyperfine" "$PKGDIR/bin/hyperfine"; chmod +x "$PKGDIR/bin/hyperfine"' "bin/hyperfine"

# tokei (code stats)
build_pkg "tokei" "13.0.0-alpha.7" "https://github.com/XAMPPRocky/tokei/releases/download/v13.0.0-alpha.7/tokei-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/tokei" "$PKGDIR/bin/tokei"; chmod +x "$PKGDIR/bin/tokei"' "bin/tokei"

# sd (sed alternative)
build_pkg "sd" "1.0.0" "https://github.com/chmln/sd/releases/download/v1.0.0/sd-v1.0.0-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; find "$SRCDIR" -name sd -type f | head -1 | xargs -I{} cp {} "$PKGDIR/bin/sd"; chmod +x "$PKGDIR/bin/sd"' "bin/sd"

# bandwhich (network monitor)
build_pkg "bandwhich" "0.22.2" "https://github.com/imsnif/bandwhich/releases/download/v0.22.2/bandwhich-v0.22.2-x86_64-unknown-linux-musl.tar.gz" \
    'mkdir -p "$PKGDIR/bin"; find "$SRCDIR" -name bandwhich -type f | head -1 | xargs -I{} cp {} "$PKGDIR/bin/bandwhich"; chmod +x "$PKGDIR/bin/bandwhich"' "bin/bandwhich"

# dog (DNS client, dig alternative)
build_pkg "dog" "0.1.0" "https://github.com/ogham/dog/releases/download/v0.1.0/dog-v0.1.0-x86_64-unknown-linux-musl.zip" \
    'mkdir -p "$PKGDIR/bin"; find "$SRCDIR" -name dog -type f | head -1 | xargs -I{} cp {} "$PKGDIR/bin/dog"; chmod +x "$PKGDIR/bin/dog"' "bin/dog"

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
# Create a welcome script (avoids escape issues in unit files)
cat > "$INITRD/bin/bingux-welcome" << 'WELCOMESCRIPT'
#!/bin/sh
echo ""
echo "  Bingux Live Environment (systemd)"
echo "  ================================="
echo ""
export BPKG_STORE_ROOT=/system/packages
export PATH=/system/profiles/default/bin:/bin:/usr/bin
export LD_LIBRARY_PATH=/lib64:/usr/lib64
echo "  Packages:"
/bin/bpkg list 2>&1 | sed 's/^/    /'
echo ""
echo "  Ready."
WELCOMESCRIPT
chmod +x "$INITRD/bin/bingux-welcome"

cat > "$INITRD/usr/lib/systemd/system/bingux-welcome.service" << 'UNIT'
[Unit]
Description=Bingux Welcome Message
After=systemd-journald.service

[Service]
Type=oneshot
ExecStart=/bin/bingux-welcome
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
