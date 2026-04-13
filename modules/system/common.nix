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
        services.automatic-timezoned.enable = lib.mkDefault true;
        services.openssh.enable = lib.mkDefault false;
        services.geoclue2.enable = lib.mkDefault true;
        services.earlyoom.enable = lib.mkDefault true;
        location.provider = lib.mkDefault "geoclue2";

        hardware.bluetooth.enable = lib.mkDefault true;
        hardware.bluetooth.powerOnBoot = lib.mkDefault true;

        # Printing
        services.printing.enable = lib.mkDefault true;
        services.printing.drivers = [ pkgs.gutenprint pkgs.hplip ];

        # mDNS / Bonjour
        services.avahi = {
            enable = lib.mkDefault true;
            nssmdns4 = lib.mkDefault true;
            openFirewall = lib.mkDefault true;
            publish = {
                enable = lib.mkDefault true;
                addresses = lib.mkDefault true;
                workstation = lib.mkDefault true;
            };
        };

        i18n.defaultLocale = lib.mkDefault "en_US.UTF-8";
    };
}
