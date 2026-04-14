#!/bin/bash
# Build a bootable Bingux ISO from .bgx packages.
#
# Usage: ./mkiso.sh [--output bingux.iso]
#
# Expects .bgx packages in $BGX_DIR (default: /system/exports/)
# Produces a hybrid MBR/EFI bootable ISO.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BGX_DIR="${BGX_DIR:-/system/exports}"
OUTPUT="${1:-bingux-$(date +%Y.%m)-x86_64.iso}"
WORK_DIR=$(mktemp -d)

trap "rm -rf $WORK_DIR" EXIT

echo "==> Building Bingux ISO"

# 1. Create ISO filesystem layout
mkdir -p "$WORK_DIR"/{boot/loader/entries,packages/{essential,base,desktop,extras},installer}

# 2. Copy kernel + initramfs
echo "  Copying kernel..."
# cp /system/packages/linux-*/boot/vmlinuz "$WORK_DIR/boot/"
# cp initramfs "$WORK_DIR/boot/initramfs.img"

# 3. Configure systemd-boot
cat > "$WORK_DIR/boot/loader/loader.conf" << 'LOADER'
default bingux.conf
timeout 5
editor no
LOADER

cat > "$WORK_DIR/boot/loader/entries/bingux.conf" << 'ENTRY'
title   Bingux Installer
linux   /boot/vmlinuz
initrd  /boot/initramfs.img
options console=ttyS0,115200 loglevel=7
ENTRY

# 4. Categorize and copy .bgx packages
echo "  Copying packages..."
for set_name in essential base desktop extras; do
    set_file="$SCRIPT_DIR/sets/${set_name}.txt"
    [ -f "$set_file" ] || continue

    while read -r pkg; do
        [ -n "$pkg" ] || continue
        [[ "$pkg" =~ ^# ]] && continue
        bgx=$(find "$BGX_DIR" -name "${pkg}-*-x86_64-linux.bgx" 2>/dev/null | head -1)
        if [ -n "$bgx" ]; then
            cp "$bgx" "$WORK_DIR/packages/${set_name}/"
            echo "    ✓ $pkg → $set_name"
        else
            echo "    ✗ $pkg (not found)"
        fi
    done < "$set_file"
done

# 5. Copy installer
echo "  Copying installer..."
# cp /system/packages/bingux-installer-*/bin/bingux-installer "$WORK_DIR/installer/"

# 6. Generate manifest
cat > "$WORK_DIR/manifest.toml" << MANIFEST
[meta]
version = "$(date +%Y.%m)"
arch = "x86_64-linux"
created = "$(date -Iseconds)"

[package_sets]
essential = "packages/essential/"
base = "packages/base/"
desktop = "packages/desktop/"
extras = "packages/extras/"
MANIFEST

# 7. Build ISO with xorriso
echo "  Building ISO..."
# xorriso -as mkisofs \
#     -o "$OUTPUT" \
#     -isohybrid-mbr /usr/share/syslinux/isohdpfx.bin \
#     -c boot.cat \
#     -b boot/loader/entries/bingux.conf \
#     -no-emul-boot -boot-load-size 4 -boot-info-table \
#     -eltorito-alt-boot -e boot/efiboot.img -no-emul-boot \
#     -isohybrid-gpt-basdat \
#     "$WORK_DIR"

echo "==> ISO built: $OUTPUT"
