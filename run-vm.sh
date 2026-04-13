#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "$0")" && pwd)
TARGET_HOST="${1:-fsociety}"
DISK_PATH="$ROOT_DIR/.cache/qemu/${TARGET_HOST}-installer.qcow2"

if [[ ! -f "$DISK_PATH" ]]; then
    echo "No disk found at $DISK_PATH" >&2
    echo "Install first with: ./run-iso.sh $TARGET_HOST" >&2
    exit 1
fi

ACCEL_ARGS=( -accel tcg -cpu max )
if [[ -r /dev/kvm && -w /dev/kvm ]]; then
    ACCEL_ARGS=( -enable-kvm -cpu host )
fi

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

# Share the repo with the VM via 9p so you can update /os without rebuilding ISO
echo ":: Booting installed VM (no ISO)"
echo ":: Disk: $DISK_PATH"
echo ":: Shared folder: $ROOT_DIR -> /mnt/host (mount with: sudo mount -t 9p host /mnt/host -o trans=virtio)"

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
    -virtfs local,path="$ROOT_DIR",mount_tag=host,security_model=mapped-xattr,id=host
