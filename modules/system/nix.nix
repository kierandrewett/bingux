{ lib, ... }:
{
    nix.settings = {
        experimental-features = [ "nix-command" "flakes" ];
        auto-optimise-store = lib.mkDefault true;
        trusted-users = lib.mkDefault [ "root" "@wheel" ];
        warn-dirty = lib.mkDefault false;
    };

    nix.gc = {
        automatic = lib.mkDefault true;
        dates = lib.mkDefault "weekly";
        options = lib.mkDefault "--delete-older-than 14d";
    };

    nix.channel.enable = lib.mkDefault false;

    system.autoUpgrade = {
        enable = lib.mkDefault false;
    };
}
