{ config, lib, pkgs, ... }:
let
    isKde = config.bingux.desktop == "kde";
in
{
    config = lib.mkIf isKde {
        services.xserver.enable = lib.mkDefault true;
        services.displayManager.sddm.enable = lib.mkDefault true;
        services.displayManager.sddm.wayland.enable = lib.mkDefault true;
        services.desktopManager.plasma6.enable = lib.mkDefault true;
        services.displayManager.defaultSession = lib.mkDefault "plasma";

        environment.systemPackages = with pkgs; [
            kdePackages.kate
            kdePackages.ark
            kdePackages.kcalc
            kdePackages.filelight
            kdePackages.spectacle
            kdePackages.gwenview
            kdePackages.okular
        ];
    };
}
