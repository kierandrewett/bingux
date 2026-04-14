#!/bin/bash
# Quick-launch a Bingux VM for testing.
# Usage: ./launch.sh <iso-or-qcow2>
set -euo pipefail

IMAGE="${1:?Usage: launch.sh <iso-or-qcow2>}"
MEMORY="${MEMORY:-4G}"
CPUS="${CPUS:-4}"

QEMU_ARGS=(
    -enable-kvm
    -m "$MEMORY"
    -smp "$CPUS"
    -device virtio-gpu-pci
    -device virtio-keyboard-pci
    -device virtio-mouse-pci
    -device virtio-net-pci,netdev=net0
    -netdev user,id=net0
    -serial unix:/tmp/bingux-serial.sock,server,nowait
    -qmp unix:/tmp/bingux-qmp.sock,server,nowait
    -bios /usr/share/OVMF/OVMF_CODE.fd
)

case "$IMAGE" in
    *.iso)
        QEMU_ARGS+=(-cdrom "$IMAGE" -boot d)
        QEMU_ARGS+=(-drive file=/tmp/bingux-test.qcow2,format=qcow2,if=virtio)
        # Create test disk if needed
        [ -f /tmp/bingux-test.qcow2 ] || qemu-img create -f qcow2 /tmp/bingux-test.qcow2 64G
        ;;
    *.qcow2)
        QEMU_ARGS+=(-drive file="$IMAGE",format=qcow2,if=virtio)
        ;;
esac

echo "Launching QEMU..."
echo "  Serial: /tmp/bingux-serial.sock"
echo "  QMP:    /tmp/bingux-qmp.sock"
qemu-system-x86_64 "${QEMU_ARGS[@]}"
