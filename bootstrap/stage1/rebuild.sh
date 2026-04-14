#!/bin/bash
# Stage 1: Using the stage0 tools, build core system packages from recipes.
#
# This stage uses the statically-linked bpkg/bsys from stage0 to
# process BPKGBUILD recipes and produce proper packages in /system/packages/.
# The resulting packages are dynamically linked against glibc.

set -euo pipefail

STAGE1_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$STAGE1_DIR/../.." && pwd)"
RECIPES_DIR="$ROOT_DIR/recipes"
SYSROOT="${SYSROOT:-/system}"
BPKG="${SYSROOT}/packages/bpkg-0.1.0-x86_64-linux/bin/bpkg"

if [ ! -x "$BPKG" ]; then
    echo "Error: stage0 bpkg not found at $BPKG"
    echo "Run stage0/build.sh && stage0/seed-packages.sh first."
    exit 1
fi

echo "==> Stage 1: Building core packages from recipes"

# Build order matters: dependencies must be built first
CORE_PACKAGES=(
    "core/glibc"
    "core/linux"
    "core/bash"
    "core/coreutils"
    "core/systemd"
)

BUILD_PACKAGES=(
    "build/binutils"
    "build/gcc"
    "build/make"
    "build/rust"
)

TOOLCHAIN_PACKAGES=(
    "toolchain/bpkg"
    "toolchain/bsys"
    "toolchain/bxc"
    "toolchain/bingux-gated"
)

build_recipe() {
    local recipe="$1"
    local recipe_dir="$RECIPES_DIR/$recipe"

    if [ ! -f "$recipe_dir/BPKGBUILD" ]; then
        echo "  SKIP $recipe (no BPKGBUILD)"
        return 0
    fi

    echo "  Building $recipe..."
    "$BPKG" build "$recipe_dir" --sysroot "$SYSROOT" 2>&1 | sed 's/^/    /' || {
        echo "  FAIL $recipe"
        return 1
    }
    echo "  OK   $recipe"
}

FAILED=()

for pkg in "${CORE_PACKAGES[@]}" "${BUILD_PACKAGES[@]}" "${TOOLCHAIN_PACKAGES[@]}"; do
    build_recipe "$pkg" || FAILED+=("$pkg")
done

echo ""
echo "==> Stage 1 complete."
echo "    Built: $(( ${#CORE_PACKAGES[@]} + ${#BUILD_PACKAGES[@]} + ${#TOOLCHAIN_PACKAGES[@]} - ${#FAILED[@]} )) packages"

if [ ${#FAILED[@]} -gt 0 ]; then
    echo "    Failed: ${FAILED[*]}"
    exit 1
fi
