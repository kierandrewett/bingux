{ config, lib, pkgs, ... }:
let
    cfg = config.bingux.boot;
    bingux-plymouth = pkgs.callPackage ../../pkgs/bingux-plymouth { };
in
{
    options.bingux.boot.luksUuid = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "LUKS UUID for cryptroot (null disables initrd cryptroot mapping).";
    };

    config = {
        boot.loader = {
            systemd-boot.enable = lib.mkDefault true;
            efi.canTouchEfiVariables = lib.mkDefault true;
            timeout = lib.mkDefault 0;
        };

        boot.initrd = {
            systemd.enable = lib.mkDefault true;
            luks.devices = lib.mkIf (cfg.luksUuid != null) {
                "cryptroot" = {
                    device = "/dev/disk/by-uuid/${cfg.luksUuid}";
                    allowDiscards = true;
                    preLVM = true;
                };
            };
            availableKernelModules = [
                "nvme"
                "xhci_pci"
                "ahci"
                "usbhid"
                "sd_mod"
                "amdgpu"
            ];
            verbose = lib.mkDefault false;
        };

        console.keyMap = lib.mkDefault "us";

        boot.plymouth.font = lib.mkDefault
            "${pkgs.adwaita-fonts}/share/fonts/Adwaita/AdwaitaSans-Regular.ttf";

        # Console (TTY) font
        console.packages = [ pkgs.terminus_font ];
        console.font = lib.mkDefault "ter-v16n";

        boot.kernelPackages = lib.mkDefault pkgs.linuxPackages_latest;
        boot.supportedFilesystems = [ "btrfs" ];
        boot.consoleLogLevel = lib.mkDefault 0;
        boot.kernelParams = [
            "quiet"
            "splash"
            "boot.shell_on_fail"
            "loglevel=3"
            "rd.systemd.show_status=false"
            "rd.udev.log_level=3"
            "udev.log_priority=3"
        ];

        boot.plymouth = {
            enable = lib.mkDefault true;
            theme = lib.mkDefault "bingux";
            themePackages = [ bingux-plymouth ];
        };

        # Ensure plymouth password agent is explicitly enabled in initrd
        # so LUKS passphrase prompts render graphically via plymouth
        boot.initrd.systemd.paths."systemd-ask-password-plymouth" = {
            wantedBy = [ "sysinit.target" ];
        };
        boot.initrd.systemd.services."systemd-ask-password-plymouth" = {
            wantedBy = [ "sysinit.target" ];
        };
    };
}
