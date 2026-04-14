#!/bin/bash
# Export all built packages as .bgx archives for ISO inclusion.
#
# Reads /system/packages/ and creates compressed .bgx files in
# /system/exports/ ready for the ISO builder.

set -euo pipefail

SYSROOT="${SYSROOT:-/system}"
PKG_STORE="${SYSROOT}/packages"
EXPORT_DIR="${SYSROOT}/exports"
BPKG="${PKG_STORE}/bpkg-0.1.0-x86_64-linux/bin/bpkg"

if [ ! -x "$BPKG" ]; then
    echo "Error: bpkg not found at $BPKG"
    exit 1
fi

mkdir -p "$EXPORT_DIR"

echo "==> Exporting packages to $EXPORT_DIR"

count=0
for pkg_dir in "$PKG_STORE"/*/; do
    [ -f "$pkg_dir/.bpkg/manifest.toml" ] || continue
    pkg_name="$(basename "$pkg_dir")"

    echo "  Exporting $pkg_name..."
    "$BPKG" export "$pkg_dir" --output "$EXPORT_DIR/${pkg_name}.bgx" 2>&1 | sed 's/^/    /' || {
        echo "  FAIL $pkg_name"
        continue
    }
    count=$((count + 1))
done

echo "==> Exported $count packages to $EXPORT_DIR/"
