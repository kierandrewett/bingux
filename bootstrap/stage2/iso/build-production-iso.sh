#!/bin/bash
# Build a production-like Bingux ISO with the full architecture.
# Uses systemd, generation profiles, dispatch tables, and the package store.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../../.." && pwd)"
ISO_WORK=/tmp/bingux-prod-iso
INITRD="$ISO_WORK/initramfs"
ISO_ROOT="$ISO_WORK/isoroot"
STORE="$ISO_WORK/store"
CACHE="$ISO_WORK/cache"
KERNEL=$(ls /boot/vmlinuz-$(uname -r))

rm -rf "$ISO_WORK"
mkdir -p "$ISO_ROOT/boot/grub" "$STORE" "$CACHE"

echo "============================================"
echo "  Bingux Production ISO Builder"
echo "============================================"

# Phase 1: Build tools
echo "==> Building Bingux tools..."
cargo build --release --manifest-path="$ROOT_DIR/Cargo.toml" 2>&1 | tail -1

# Phase 2: Build packages using bsys build
echo "==> Building packages via bsys build..."
export BPKG_STORE_ROOT="$STORE"
export BSYS_WORK_DIR="$ISO_WORK/work"
export BSYS_CACHE_DIR="$CACHE"
mkdir -p "$BSYS_WORK_DIR"

# Self-host tools
"$ROOT_DIR/target/release/bsys-cli" build "$ROOT_DIR/recipes/toolchain/bpkg/BPKGBUILD" 2>&1 | grep -E "ok:|error"
"$ROOT_DIR/target/release/bsys-cli" build "$ROOT_DIR/recipes/toolchain/bsys/BPKGBUILD" 2>&1 | grep -E "ok:|error"
"$ROOT_DIR/target/release/bsys-cli" build "$ROOT_DIR/recipes/toolchain/bxc-shim/BPKGBUILD" 2>&1 | grep -E "ok:|error"

# Real packages
for recipe in jq ripgrep fd bat fzf; do
    recipe_dir="$ISO_WORK/recipes/$recipe"
    mkdir -p "$recipe_dir"
done

cat > "$ISO_WORK/recipes/jq/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="jq"
pkgver="1.7.1"
pkgarch="x86_64-linux"
pkgdesc="JSON processor"
license="MIT"
depends=()
exports=("bin/jq")
source=("https://github.com/jqlang/jq/releases/download/jq-1.7.1/jq-linux-amd64")
sha256sums=("SKIP")
package() { mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/jq-linux-amd64" "$PKGDIR/bin/jq"; chmod +x "$PKGDIR/bin/jq"; }
R

cat > "$ISO_WORK/recipes/ripgrep/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="ripgrep"
pkgver="14.1.1"
pkgarch="x86_64-linux"
pkgdesc="Fast search"
license="MIT"
depends=()
exports=("bin/rg")
source=("https://github.com/BurntSushi/ripgrep/releases/download/14.1.1/ripgrep-14.1.1-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")
package() { mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/ripgrep-14.1.1-x86_64-unknown-linux-musl/rg" "$PKGDIR/bin/rg"; chmod +x "$PKGDIR/bin/rg"; }
R

cat > "$ISO_WORK/recipes/fd/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="fd"
pkgver="10.2.0"
pkgarch="x86_64-linux"
pkgdesc="Fast find"
license="MIT"
depends=()
exports=("bin/fd")
source=("https://github.com/sharkdp/fd/releases/download/v10.2.0/fd-v10.2.0-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")
package() { mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fd-v10.2.0-x86_64-unknown-linux-musl/fd" "$PKGDIR/bin/fd"; chmod +x "$PKGDIR/bin/fd"; }
R

cat > "$ISO_WORK/recipes/bat/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="bat"
pkgver="0.24.0"
pkgarch="x86_64-linux"
pkgdesc="Cat with wings"
license="MIT"
depends=()
exports=("bin/bat")
source=("https://github.com/sharkdp/bat/releases/download/v0.24.0/bat-v0.24.0-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")
package() { mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/bat-v0.24.0-x86_64-unknown-linux-musl/bat" "$PKGDIR/bin/bat"; chmod +x "$PKGDIR/bin/bat"; }
R

cat > "$ISO_WORK/recipes/fzf/BPKGBUILD" << 'R'
pkgscope="bingux"
pkgname="fzf"
pkgver="0.57.0"
pkgarch="x86_64-linux"
pkgdesc="Fuzzy finder"
license="MIT"
depends=()
exports=("bin/fzf")
source=("https://github.com/junegunn/fzf/releases/download/v0.57.0/fzf-0.57.0-linux_amd64.tar.gz")
sha256sums=("SKIP")
package() { mkdir -p "$PKGDIR/bin"; cp "$SRCDIR/fzf" "$PKGDIR/bin/fzf"; chmod +x "$PKGDIR/bin/fzf"; }
R

# Additional packages — write recipes individually to avoid IFS issues with URLs
write_recipe() { local name=$1 ver=$2 url=$3 src_path=$4
    mkdir -p "$ISO_WORK/recipes/$name"
    cat > "$ISO_WORK/recipes/$name/BPKGBUILD" << RECIPE
pkgscope="bingux"
pkgname="$name"
pkgver="$ver"
pkgarch="x86_64-linux"
pkgdesc="$name"
license="MIT"
depends=()
exports=("bin/$name")
source=("$url")
sha256sums=("SKIP")
package() { mkdir -p "\$PKGDIR/bin"; cp "\$SRCDIR/$src_path" "\$PKGDIR/bin/$name"; chmod +x "\$PKGDIR/bin/$name"; }
RECIPE
}

write_recipe eza 0.20.14 "https://github.com/eza-community/eza/releases/download/v0.20.14/eza_x86_64-unknown-linux-musl.tar.gz" eza
write_recipe delta 0.18.2 "https://github.com/dandavison/delta/releases/download/0.18.2/delta-0.18.2-x86_64-unknown-linux-musl.tar.gz" "delta-0.18.2-x86_64-unknown-linux-musl/delta"
write_recipe zoxide 0.9.6 "https://github.com/ajeetdsouza/zoxide/releases/download/v0.9.6/zoxide-0.9.6-x86_64-unknown-linux-musl.tar.gz" zoxide
write_recipe dust 1.1.1 "https://github.com/bootandy/dust/releases/download/v1.1.1/dust-v1.1.1-x86_64-unknown-linux-musl.tar.gz" "dust-v1.1.1-x86_64-unknown-linux-musl/dust"
write_recipe starship 1.22.1 "https://github.com/starship/starship/releases/download/v1.22.1/starship-x86_64-unknown-linux-musl.tar.gz" starship
write_recipe hexyl 0.14.0 "https://github.com/sharkdp/hexyl/releases/download/v0.14.0/hexyl-v0.14.0-x86_64-unknown-linux-musl.tar.gz" "hexyl-v0.14.0-x86_64-unknown-linux-musl/hexyl"
write_recipe hyperfine 1.19.0 "https://github.com/sharkdp/hyperfine/releases/download/v1.19.0/hyperfine-v1.19.0-x86_64-unknown-linux-musl.tar.gz" "hyperfine-v1.19.0-x86_64-unknown-linux-musl/hyperfine"

for recipe in jq ripgrep fd bat fzf eza delta zoxide dust starship hexyl hyperfine; do
    "$ROOT_DIR/target/release/bsys-cli" build "$ISO_WORK/recipes/$recipe/BPKGBUILD" 2>&1 | grep -E "ok:|error"
done

echo ""
echo "    Store: $(ls "$STORE" | wc -l) packages"

# Phase 3: Compose generation using bsys apply
echo "==> Composing generation..."
export BSYS_PROFILES_ROOT="$ISO_WORK/profiles"
export BSYS_PACKAGES_ROOT="$STORE"
export BSYS_CONFIG_PATH="$ISO_WORK/system.toml"
export BSYS_ETC_ROOT="$ISO_WORK/etc-gen"
mkdir -p "$BSYS_PROFILES_ROOT" "$BSYS_ETC_ROOT"

# Write system.toml with all packages
KEEP_LIST=$(ls "$STORE" | sed 's/-[0-9].*//' | sort -u | tr '\n' ',' | sed 's/,$//' | sed 's/,/", "/g')
cat > "$ISO_WORK/system.toml" << SYSCONF
[system]
hostname = "bingux"
locale = "en_GB.UTF-8"
timezone = "Europe/London"
keymap = "uk"

[packages]
keep = ["$KEEP_LIST"]

[services]
enable = []
SYSCONF

"$ROOT_DIR/target/release/bsys-cli" apply 2>&1 | grep -E "apply:|etc:|error"

# Phase 4: Build initramfs
echo "==> Building initramfs..."
mkdir -p "$INITRD"/{bin,sbin,lib,lib64,usr/{bin,lib,lib64,sbin},dev,proc,sys,run,tmp,etc/ssl/certs}
mkdir -p "$INITRD"/system/{packages,profiles,config,state}
mkdir -p "$INITRD"/var/{log/journal,run,tmp,lib/systemd}
mkdir -p "$INITRD"/run/{systemd/system,dbus,lock,user}

cp "$KERNEL" "$ISO_ROOT/boot/vmlinuz"

# Busybox
cp /usr/bin/busybox "$INITRD/bin/"
for cmd in sh cat echo ls mkdir mount umount wc head sort grep cp rm ln chmod printf find reboot poweroff; do
    ln -sf busybox "$INITRD/bin/$cmd"
done
for cmd in sh mount umount; do
    mkdir -p "$INITRD/usr/bin" "$INITRD/usr/sbin"
    ln -sf ../../bin/busybox "$INITRD/usr/bin/$cmd"
    ln -sf ../../bin/busybox "$INITRD/usr/sbin/$cmd"
done

# Libs for dynamically linked binaries
for lib in libc.so.6 libm.so.6 libgcc_s.so.1 ld-linux-x86-64.so.2 libbz2.so.1 libcrypto.so.3 libssl.so.3 libz.so.1; do
    src=$(find /lib64 /usr/lib64 -name "$lib" -maxdepth 1 2>/dev/null | head -1)
    [ -n "$src" ] && cp -L "$src" "$INITRD/lib64/"
done

# SSL certs
cp /etc/ssl/certs/ca-bundle.crt "$INITRD/etc/ssl/certs/" 2>/dev/null || true

# Copy package store
cp -a "$STORE"/* "$INITRD/system/packages/"

# Create generation profile with correct runtime paths (/system/packages/...)
# Don't copy the build-time generation — it has wrong symlink targets
PROFILE_DIR="$INITRD/system/profiles/1"
PROFILE_BIN="$PROFILE_DIR/bin"
mkdir -p "$PROFILE_BIN" "$PROFILE_DIR/lib" "$PROFILE_DIR/share"

# Create dispatch table and symlinks for all packages
for pkg_dir in "$INITRD/system/packages"/*/; do
    [ -d "$pkg_dir" ] || continue
    pkg_name=$(basename "$pkg_dir")
    manifest="$pkg_dir/.bpkg/manifest.toml"
    [ -f "$manifest" ] || continue

    # Symlink all binaries in the package
    if [ -d "$pkg_dir/bin" ]; then
        for binary in "$pkg_dir/bin"/*; do
            [ -f "$binary" ] || continue
            bin_name=$(basename "$binary")
            # Use absolute runtime path
            ln -sf "/system/packages/$pkg_name/bin/$bin_name" "$PROFILE_BIN/$bin_name"
        done
    fi
done

# Copy dispatch table from build-time generation if it exists
if [ -f "$BSYS_PROFILES_ROOT/1/.dispatch.toml" ]; then
    cp "$BSYS_PROFILES_ROOT/1/.dispatch.toml" "$PROFILE_DIR/.dispatch.toml"
fi
if [ -f "$BSYS_PROFILES_ROOT/1/generation.toml" ]; then
    cp "$BSYS_PROFILES_ROOT/1/generation.toml" "$PROFILE_DIR/generation.toml"
fi

# Create current symlink
ln -sf 1 "$INITRD/system/profiles/current"

echo "    Profile bin/ entries: $(ls "$PROFILE_BIN" | wc -l)"

# System config
cp "$ISO_WORK/system.toml" "$INITRD/system/config/system.toml"

# Generated /etc files
for f in "$BSYS_ETC_ROOT"/*; do
    [ -f "$f" ] && cp "$f" "$INITRD/etc/"
done

# systemd
copy_with_deps() {
    local binary="$1"
    [ -f "$binary" ] || return
    local dest="$INITRD/$binary"
    mkdir -p "$(dirname "$dest")"
    cp -L "$binary" "$dest" 2>/dev/null || true
    ldd "$binary" 2>/dev/null | grep "=> /" | awk '{print $3}' | while read -r lib; do
        local ldest="$INITRD/$lib"
        mkdir -p "$(dirname "$ldest")"
        [ -f "$ldest" ] || cp -L "$lib" "$ldest" 2>/dev/null || true
    done
}

for bin in /usr/lib/systemd/systemd /usr/lib/systemd/systemd-executor /usr/lib/systemd/systemd-journald \
    /usr/bin/systemctl /usr/bin/journalctl; do
    copy_with_deps "$bin"
done
mkdir -p "$INITRD/usr/lib64/systemd"
cp -L /usr/lib64/systemd/*.so* "$INITRD/usr/lib64/systemd/" 2>/dev/null || true

# systemd units
mkdir -p "$INITRD/usr/lib/systemd/system"
for unit in basic.target sysinit.target multi-user.target default.target paths.target slices.target \
    sockets.target swap.target timers.target local-fs.target local-fs-pre.target tmp.mount \
    systemd-journald.service systemd-journald.socket systemd-journald-dev-log.socket \
    systemd-tmpfiles-setup.service systemd-tmpfiles-setup-dev-early.service \
    systemd-sysctl.service systemd-remount-fs.service systemd-update-utmp.service; do
    [ -f "/usr/lib/systemd/system/$unit" ] && cp "/usr/lib/systemd/system/$unit" "$INITRD/usr/lib/systemd/system/"
done
for wants in /usr/lib/systemd/system/*.target.wants; do
    [ -d "$wants" ] || continue
    mkdir -p "$INITRD/$wants"
    cp -a "$wants"/* "$INITRD/$wants/" 2>/dev/null || true
done
ln -sf multi-user.target "$INITRD/usr/lib/systemd/system/default.target"

# systemd helper binaries
for helper in systemd-tmpfiles systemd-sysctl systemd-remount-fs systemd-update-utmp systemd-modules-load; do
    for loc in /usr/bin/$helper /usr/lib/systemd/$helper; do
        [ -f "$loc" ] && copy_with_deps "$loc"
    done
done

# tmpfiles.d
mkdir -p "$INITRD/usr/lib/tmpfiles.d"
for f in /usr/lib/tmpfiles.d/{systemd,systemd-tmp,tmp,var}.conf; do
    [ -f "$f" ] && cp "$f" "$INITRD/usr/lib/tmpfiles.d/"
done

# /etc essentials
touch "$INITRD/etc/machine-id"
cat > "$INITRD/etc/os-release" << 'OSREL'
NAME="Bingux"
ID=bingux
VERSION="0.1.0"
PRETTY_NAME="Bingux 0.1.0"
HOME_URL="https://github.com/kierandrewett/bingux"
OSREL

cat > "$INITRD/etc/passwd" << 'P'
root:x:0:0:root:/root:/bin/sh
bingux:x:1000:1000:Bingux User:/tmp:/bin/sh
P
cat > "$INITRD/etc/group" << 'G'
root:x:0:
wheel:x:10:bingux
bingux:x:1000:
G
cat > "$INITRD/etc/shadow" << 'S'
root::0:0:99999:7:::
bingux::0:0:99999:7:::
S
chmod 600 "$INITRD/etc/shadow"

# Bingux welcome service
cat > "$INITRD/bin/bingux-welcome" << 'W'
#!/bin/sh
echo ""
echo "  Bingux 0.1.0 — powered by systemd"
echo "  ================================="
echo ""
echo "  Packages:"
BPKG_STORE_ROOT=/system/packages PATH=/system/profiles/current/bin:/bin:/usr/bin LD_LIBRARY_PATH=/lib64:/usr/lib64 \
    /system/packages/bpkg-0.1.0-x86_64-linux/bin/bpkg list 2>&1 | while read -r line; do echo "    $line"; done
echo ""
echo "  Tools available via /system/profiles/current/bin/:"
ls /system/profiles/current/bin/ 2>/dev/null | tr '\n' ' '
echo ""
echo ""
echo "  Ready. Use jq/rg/fd/bat/fzf or bpkg to manage packages."
W
chmod +x "$INITRD/bin/bingux-welcome"

mkdir -p "$INITRD/usr/lib/systemd/system/multi-user.target.wants"
cat > "$INITRD/usr/lib/systemd/system/bingux-welcome.service" << 'UNIT'
[Unit]
Description=Bingux Welcome
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

# Shell service
cat > "$INITRD/usr/lib/systemd/system/bingux-shell.service" << 'UNIT'
[Unit]
Description=Bingux Shell
After=bingux-welcome.service
[Service]
Type=idle
ExecStart=/bin/sh -l
StandardInput=tty
StandardOutput=tty
StandardError=tty
TTYPath=/dev/ttyS0
TTYReset=yes
Environment=PATH=/system/profiles/current/bin:/bin:/usr/bin:/sbin
Environment=LD_LIBRARY_PATH=/lib64:/usr/lib64
Environment=BPKG_STORE_ROOT=/system/packages
Environment=HOME=/tmp
Environment=TERM=linux
[Install]
WantedBy=multi-user.target
UNIT
ln -sf ../bingux-shell.service "$INITRD/usr/lib/systemd/system/multi-user.target.wants/"

# Init → systemd
ln -sf /usr/lib/systemd/systemd "$INITRD/init"
ln -sf /usr/lib/systemd/systemd "$INITRD/sbin/init"

# Pack
echo "    Initramfs files: $(find "$INITRD" -type f | wc -l)"
(cd "$INITRD" && find . | cpio -o -H newc 2>/dev/null | gzip > "$ISO_ROOT/boot/initramfs.img")
echo "    Initramfs: $(du -h "$ISO_ROOT/boot/initramfs.img" | cut -f1)"

# GRUB config
cat > "$ISO_ROOT/boot/grub/grub.cfg" << 'GRUB'
set timeout=3
menuentry "Bingux" {
    linux /boot/vmlinuz rdinit=/usr/lib/systemd/systemd console=ttyS0,115200 console=tty0 selinux=0 quiet
    initrd /boot/initramfs.img
}
GRUB

# Build ISO
genisoimage -o /tmp/bingux-prod.iso -R -J -V "BINGUX" "$ISO_ROOT" 2>/dev/null

echo ""
echo "============================================"
echo "  Production ISO: /tmp/bingux-prod.iso"
echo "  Size: $(du -h /tmp/bingux-prod.iso | cut -f1)"
echo "  Packages: $(ls "$STORE" | wc -l)"
echo "============================================"
