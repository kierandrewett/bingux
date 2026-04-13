{ ... }:
{
    nix.settings = {
        experimental-features = [ "nix-command" "flakes" ];
        auto-optimise-store = true;
        trusted-users = [ "root" "@wheel" ];
        warn-dirty = false;
    };

    nix.gc = {
        automatic = true;
        dates = "weekly";
        options = "--delete-older-than 14d";
    };

    nix.channel.enable = false;

    system.autoUpgrade = {
        enable = false;
    };
}
