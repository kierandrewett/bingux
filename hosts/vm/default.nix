{ lib, modulesPath, ... }:
{
    imports = [
        "${modulesPath}/virtualisation/qemu-vm.nix"
    ];

    boot.initrd.availableKernelModules = [
        "virtio_blk"
        "virtio_net"
        "virtio_pci"
    ];

    boot.kernelModules = [
        "virtio_balloon"
        "virtio_gpu"
    ];

    networking.useDHCP = lib.mkDefault true;
    services.qemuGuest.enable = true;

    virtualisation.vmVariant = {
        virtualisation = {
            cores = 8;
            graphics = true;
            memorySize = 8192;
        };
    };
}
