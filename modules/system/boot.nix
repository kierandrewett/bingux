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
            systemd-boot.enable = true;
            efi.canTouchEfiVariables = true;
            timeout = 0;
        };

        boot.initrd = {
            systemd.enable = true;
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
            verbose = false;
        };

        # GB keyboard layout (also applied in initrd for LUKS passphrase entry)
        console.keyMap = "uk";

        # Include fonts in Plymouth for LUKS prompt
        # Plymouth (LUKS prompt) uses sans-serif
        boot.plymouth.font = "${pkgs.adwaita-fonts}/share/fonts/Adwaita/AdwaitaSans-Regular.ttf";

        # Console (TTY) font
        console.packages = [ pkgs.terminus_font ];
        console.font = "ter-v16n";

        boot.kernelPackages = pkgs.linuxPackages_latest;
        boot.supportedFilesystems = [ "btrfs" ];
        boot.consoleLogLevel = 0;
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
            enable = true;
            theme = "bingux";
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
