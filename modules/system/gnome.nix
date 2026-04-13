{ pkgs, ... }:
{
    # Auto-unlock gnome-keyring on login
    security.pam.services.gdm-password.enableGnomeKeyring = true;
    services.gnome.gnome-keyring.enable = true;

    services.xserver = {
        enable = true;
        displayManager.gdm.enable = true;
        desktopManager.gnome.enable = true;
    };

    services.displayManager.defaultSession = "gnome";

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
