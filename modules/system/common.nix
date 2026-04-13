{ lib, pkgs, ... }:
{
    imports = [
        ./monitors.nix
        ./nix.nix
        ./boot.nix
        ./branding.nix
        ./audio.nix
        ./gnome.nix
        ./fonts.nix
        ./gpg.nix
    ];

    config = {
        services.automatic-timezoned.enable = true;
        services.openssh.enable = true;
        services.geoclue2.enable = true;
        services.earlyoom.enable = true;
        location.provider = "geoclue2";

        hardware.bluetooth.enable = true;
        hardware.bluetooth.powerOnBoot = true;

        # Printing
        services.printing.enable = true;
        services.printing.drivers = [ pkgs.gutenprint pkgs.hplip ];

        # mDNS / Bonjour
        services.avahi = {
            enable = true;
            nssmdns4 = true;
            openFirewall = true;
            publish = {
                enable = true;
                addresses = true;
                workstation = true;
            };
        };

        i18n.defaultLocale = "en_GB.UTF-8";
    };
}
