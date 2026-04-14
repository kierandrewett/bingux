#!/bin/bash
# Seed the package store with stage0 tools.
# Creates the initial package directory structure that bpkg expects.
#
# This produces a minimal /system/packages/ tree with manifest files
# so that stage1 can use bpkg to resolve dependencies.

set -euo pipefail

STAGE0_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT_DIR="${STAGE0_DIR}/output"
SYSROOT="${SYSROOT:-/system}"
PKG_STORE="${SYSROOT}/packages"

if [ ! -d "$OUTPUT_DIR" ] || [ -z "$(ls -A "$OUTPUT_DIR" 2>/dev/null)" ]; then
    echo "Error: No stage0 binaries found in $OUTPUT_DIR"
    echo "Run stage0/build.sh first."
    exit 1
fi

echo "==> Seeding package store at $PKG_STORE"

for binary in bpkg bsys bxc-shim; do
    src="$OUTPUT_DIR/$binary"
    [ -f "$src" ] || continue

    pkg_dir="$PKG_STORE/${binary}-0.1.0-x86_64-linux"
    mkdir -p "$pkg_dir/bin" "$pkg_dir/.bpkg"

    cp "$src" "$pkg_dir/bin/$binary"
    chmod +x "$pkg_dir/bin/$binary"

    # Write package manifest
    cat > "$pkg_dir/.bpkg/manifest.toml" << EOF
[package]
scope = "bingux"
name = "$binary"
version = "0.1.0"
arch = "x86_64-linux"
description = "Bingux $binary (bootstrap stage0)"

[build]
stage = "stage0"
static = true
timestamp = "$(date -Iseconds)"

[exports]
bins = ["bin/$binary"]
EOF

    echo "  ✓ $binary → $pkg_dir"
done

# Create the package store index
cat > "$PKG_STORE/.store-meta.toml" << EOF
[store]
format = 1
created = "$(date -Iseconds)"
stage = "stage0"
EOF

echo "==> Package store seeded with $(ls -d "$PKG_STORE"/*-*-* 2>/dev/null | wc -l) packages"
