#!/bin/bash
# Stage 0: Cross-compile Bingux tools using the host system.
# Produces statically-linked musl binaries that can run anywhere.
# These bootstrap binaries are used to build Stage 1.

set -euo pipefail

STAGE0_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$STAGE0_DIR/../.." && pwd)"
OUTPUT_DIR="${STAGE0_DIR}/output"

echo "==> Stage 0: Cross-compiling Bingux toolchain (static musl)"

# Ensure musl target is available
rustup target add x86_64-unknown-linux-musl

mkdir -p "$OUTPUT_DIR"

# Build all Bingux tools as static binaries
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc 2>/dev/null || true

for binary in bpkg bsys bxc-shim; do
    echo "  Building $binary..."
    cargo build --release --target x86_64-unknown-linux-musl \
        --manifest-path "$ROOT_DIR/cli/$binary/Cargo.toml" 2>/dev/null || \
    cargo build --release --target x86_64-unknown-linux-musl \
        -p "${binary}-cli" 2>/dev/null || \
    cargo build --release --target x86_64-unknown-linux-musl \
        -p "$binary" || true
done

# Copy outputs
for binary in bpkg bsys bxc-shim; do
    src="$ROOT_DIR/target/x86_64-unknown-linux-musl/release/$binary"
    if [ -f "$src" ]; then
        cp "$src" "$OUTPUT_DIR/"
        echo "  ✓ $binary → $OUTPUT_DIR/$binary"
    fi
done

echo "==> Stage 0 complete. Binaries in $OUTPUT_DIR/"
