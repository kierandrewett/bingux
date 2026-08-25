{ lib, ... }:
{
    imports = [
        ./vm
    ];

    # The Proxmox VM does not provide the 9p devices created by qemu-vm.nix.
    # Keep the shared directories disabled so boot does not enter emergency mode.
    virtualisation.mountHostNixStore = false;
    virtualisation.sharedDirectories = lib.mkForce { };

    # Proxmox presents the validation disk as /dev/sda rather than the
    # virtio-root device used by the standalone qemu-vm output.
    virtualisation.bootLoaderDevice = lib.mkForce "/dev/sda";
    # Proxmox validation boots this GPT disk with OVMF. Install GRUB in the
    # removable EFI path so the VM does not need persistent EFI variables.
    boot.loader.grub = {
        efiSupport = lib.mkForce true;
        efiInstallAsRemovable = lib.mkForce true;
        devices = lib.mkForce [ "nodev" ];
    };
    boot.loader.efi.canTouchEfiVariables = lib.mkForce false;
    fileSystems."/boot" = {
        device = "/dev/sda1";
        fsType = "vfat";
        options = [ "umask=0077" ];
    };
}
