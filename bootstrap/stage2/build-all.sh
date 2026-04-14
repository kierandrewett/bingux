#!/bin/bash
# Stage 2: Build the complete system using stage1 packages.
#
# At this point we have a self-hosting toolchain. This stage rebuilds
# everything from source using the stage1 tools (which are dynamically
# linked and fully functional), then builds the desktop/extra packages.

set -euo pipefail

STAGE2_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$STAGE2_DIR/../.." && pwd)"
RECIPES_DIR="$ROOT_DIR/recipes"
SYSROOT="${SYSROOT:-/system}"
BSYS="${SYSROOT}/packages/bsys-0.1.0-x86_64-linux/bin/bsys"

if [ ! -x "$BSYS" ]; then
    echo "Error: stage1 bsys not found at $BSYS"
    echo "Run stage1/rebuild.sh first."
    exit 1
fi

echo "==> Stage 2: Full system build"

# Rebuild everything for reproducibility
echo "  Phase 1: Rebuilding core + build + toolchain packages..."
"$BSYS" rebuild --sysroot "$SYSROOT" --recipes "$RECIPES_DIR" --all 2>&1 | sed 's/^/    /' || true

# Build additional packages (desktop, extras)
echo "  Phase 2: Building additional packages..."
for category in desktop extras; do
    if [ -d "$RECIPES_DIR/$category" ]; then
        for recipe in "$RECIPES_DIR/$category"/*/BPKGBUILD; do
            [ -f "$recipe" ] || continue
            recipe_dir="$(dirname "$recipe")"
            pkg_name="$(basename "$recipe_dir")"
            echo "    Building $category/$pkg_name..."
            "$BSYS" build "$recipe_dir" --sysroot "$SYSROOT" 2>&1 | sed 's/^/      /' || true
        done
    fi
done

echo "==> Stage 2 complete."
