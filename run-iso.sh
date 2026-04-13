#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")" && pwd)
TARGET_HOST="${1:-fsociety}"
ISO_PATH_OVERRIDE="${2:-}"
DISK_PATH="$ROOT_DIR/.cache/qemu/${TARGET_HOST}-installer.qcow2"
DISK_SIZE="${DISK_SIZE:-64G}"

if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "qemu-system-x86_64 is required." >&2
    exit 1
fi

if [[ -n "$ISO_PATH_OVERRIDE" ]]; then
    ISO_PATH=$(readlink -f "$ISO_PATH_OVERRIDE")
else
    ISO_GLOB="$ROOT_DIR/result/iso"/bingux-"$TARGET_HOST"-*.iso
    if ! ls $ISO_GLOB >/dev/null 2>&1; then
        echo "No ISO found for host $TARGET_HOST" >&2
        echo "Build one first: ./build-iso.sh $TARGET_HOST" >&2
        exit 1
    fi
    ISO_PATH=$(ls -t $ISO_GLOB | head -n 1)
fi

if [[ "$ISO_PATH" == *.zst ]]; then
    echo "Compressed ISO detected: $ISO_PATH" >&2
    echo "Decompress first: zstd -d --keep \"$ISO_PATH\"" >&2
    exit 1
fi

ACCEL_ARGS=( -accel tcg -cpu max )
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
    ACCEL_ARGS=( -enable-kvm -cpu host )
fi

# Search common OVMF locations (NixOS, Fedora/RHEL, Debian/Ubuntu, Arch)
OVMF_CODE=""
OVMF_VARS_TEMPLATE=""
for prefix in \
    /run/current-system/sw/share/OVMF \
    /usr/share/OVMF \
    /usr/share/edk2/ovmf \
    /usr/share/edk2-ovmf \
    /usr/share/edk2/x64; do
    if [[ -f "$prefix/OVMF_CODE.fd" && -f "$prefix/OVMF_VARS.fd" ]]; then
        OVMF_CODE="$prefix/OVMF_CODE.fd"
        OVMF_VARS_TEMPLATE="$prefix/OVMF_VARS.fd"
        break
    fi
done
OVMF_VARS="$ROOT_DIR/.cache/qemu/${TARGET_HOST}-ovmf-vars.fd"

UEFI_ARGS=()
if [[ -n "$OVMF_CODE" && -f "$OVMF_CODE" && -f "$OVMF_VARS_TEMPLATE" ]]; then
    mkdir -p "$(dirname "$OVMF_VARS")"
    if [[ ! -f "$OVMF_VARS" ]]; then
        cp "$OVMF_VARS_TEMPLATE" "$OVMF_VARS"
    fi
    UEFI_ARGS=(
        -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE"
        -drive if=pflash,format=raw,file="$OVMF_VARS"
    )
fi

mkdir -p "$(dirname "$DISK_PATH")"
if [[ ! -f "$DISK_PATH" ]]; then
    qemu-img create -f qcow2 "$DISK_PATH" "$DISK_SIZE" >/dev/null
fi

echo ":: Booting installer VM"
echo ":: ISO:  $ISO_PATH"
echo ":: Disk: $DISK_PATH"

qemu-system-x86_64 \
    "${ACCEL_ARGS[@]}" \
    "${UEFI_ARGS[@]}" \
    -machine q35 \
    -m 8192 \
    -smp 4 \
    -vga none \
    -device virtio-gpu-pci,xres=1280,yres=720 \
    -display gtk,gl=on \
    -audiodev pipewire,id=snd0 \
    -device intel-hda -device hda-duplex,audiodev=snd0 \
    -nic user,model=virtio-net-pci \
    -drive if=virtio,format=qcow2,file="$DISK_PATH" \
    -cdrom "$ISO_PATH" \
    -boot d
