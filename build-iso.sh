#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")" && pwd)
TARGET_HOST="fsociety"
OUTPUT_DIR="$ROOT_DIR/result/iso"
EVAL_ONLY=0
VALIDATE=0
BUILD_DATE="${BUILD_DATE:-$(date -u +%Y%m%d)}"

if git -C "$ROOT_DIR" rev-parse --verify HEAD >/dev/null 2>&1; then
    BUILD_SHA="${BUILD_SHA:-$(git -C "$ROOT_DIR" rev-parse --short=8 HEAD)}"
else
    BUILD_SHA="${BUILD_SHA:-unknown}"
fi

while [[ $# -gt 0 ]]; do
    case "$1" in
        --eval-only) EVAL_ONLY=1; shift ;;
        --validate) VALIDATE=1; shift ;;
        --host) TARGET_HOST="$2"; shift 2 ;;
        -h|--help)
            cat <<'EOF'
Usage: build-iso.sh [host] [options]

Arguments:
  host                   Target host (default: fsociety)

Options:
  --host <name>          Target host (alternative to positional arg)
  --eval-only            Evaluate derivation only, skip build
  --validate             Build system toplevel to validate config
  -h, --help             Show this help
EOF
            exit 0
            ;;
        -*) echo "Unknown option: $1" >&2; exit 1 ;;
        *) TARGET_HOST="$1"; shift ;;
    esac
done

NIX_ATTR="packages.x86_64-linux.installer-iso-${TARGET_HOST}"

echo ":: Host: $TARGET_HOST"
echo ":: Attribute: $NIX_ATTR"

if [[ "$EVAL_ONLY" == "1" ]]; then
    nix eval --raw ".#$NIX_ATTR.drvPath" >/dev/null
    echo ":: Eval OK for $NIX_ATTR"
    exit 0
fi

if [[ "$VALIDATE" == "1" ]]; then
    TOPLEVEL_ATTR="nixosConfigurations.${TARGET_HOST}.config.system.build.toplevel"
    echo ":: Validating system toplevel: $TOPLEVEL_ATTR"
    nix build --print-build-logs ".#$TOPLEVEL_ATTR" --out-link /tmp/result-validate
    echo ":: Validation OK — system toplevel builds successfully for $TARGET_HOST"
    exit 0
fi

mkdir -p "$OUTPUT_DIR"

BUILD_START=$SECONDS
echo ":: Building $NIX_ATTR ..."
nix build --print-build-logs ".#$NIX_ATTR" --out-link /tmp/result-installer-iso
BUILD_ELAPSED=$(( SECONDS - BUILD_START ))

iso_dir="/tmp/result-installer-iso/iso"
if [[ ! -d "$iso_dir" ]]; then
    echo "ERROR: build succeeded but $iso_dir does not exist" >&2
    ls -la /tmp/result-installer-iso/ >&2
    exit 1
fi

src_iso=""
for f in "$iso_dir"/*.iso; do
    if [[ -f "$f" ]]; then
        src_iso="$f"
        break
    fi
done

if [[ -z "$src_iso" ]]; then
    for f in "$iso_dir"/*.iso.zst; do
        if [[ -f "$f" ]]; then
            src_iso="$f"
            break
        fi
    done
fi

if [[ -z "$src_iso" ]]; then
    echo "ERROR: no .iso or .iso.zst found in $iso_dir" >&2
    ls -la "$iso_dir"/ >&2
    exit 1
fi

echo ":: Found ISO: $src_iso"

suffix=".iso"
case "$src_iso" in
    *.iso.zst) suffix=".iso.zst" ;;
esac
OUTPUT_NAME="bingux-${TARGET_HOST}-${BUILD_DATE}-${BUILD_SHA}-x86_64-linux${suffix}"

rm -f "$OUTPUT_DIR/$OUTPUT_NAME"
cp "$src_iso" "$OUTPUT_DIR/$OUTPUT_NAME"
chmod 644 "$OUTPUT_DIR/$OUTPUT_NAME"

echo ":: SHA256:"
sha256sum "$OUTPUT_DIR/$OUTPUT_NAME"
echo ":: ISO ready: $OUTPUT_DIR/$OUTPUT_NAME"
printf ":: Build completed in %dm%ds\n" $(( BUILD_ELAPSED / 60 )) $(( BUILD_ELAPSED % 60 ))
