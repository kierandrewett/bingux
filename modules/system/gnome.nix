{ lib, pkgs, ... }:
{
    # Auto-unlock gnome-keyring on login
    security.pam.services.gdm-password.enableGnomeKeyring = lib.mkDefault true;
    services.gnome.gnome-keyring.enable = lib.mkDefault true;

    services.xserver = {
        enable = lib.mkDefault true;
        displayManager.gdm.enable = lib.mkDefault true;
        desktopManager.gnome.enable = lib.mkDefault true;
    };

    services.displayManager.defaultSession = lib.mkDefault "gnome";

    environment.gnome.excludePackages = with pkgs; [
        epiphany
        geary
        gnome-tour
        yelp
        gnome-music
        gnome-photos
        gnome-software
    ];

    environment.systemPackages = with pkgs; [
        gnomeExtensions.appindicator
        gnome-extension-manager
        gnome-calculator
        gnome-backgrounds
        gnome-text-editor
        snapshot
        gnome-font-viewer
        evince
        loupe
        gnome-characters
        gnome-tweaks
        dconf-editor
        adw-gtk3
    ];
}
