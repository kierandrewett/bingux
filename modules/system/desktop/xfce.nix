{ config, lib, pkgs, ... }:
let
    isXfce = config.bingux.desktop == "xfce";
in
{
    config = lib.mkIf isXfce {
        services.xserver.enable = lib.mkDefault true;
        services.xserver.displayManager.lightdm.enable = lib.mkDefault true;
        services.xserver.desktopManager.xfce.enable = lib.mkDefault true;
        services.displayManager.defaultSession = lib.mkDefault "xfce";

        environment.systemPackages = with pkgs; [
            xfce.mousepad
            xfce.ristretto
            xfce.xfce4-screenshooter
            xfce.xfce4-taskmanager
            xfce.xfce4-terminal
        ];
    };
}
