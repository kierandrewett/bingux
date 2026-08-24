{ config, lib, ... }:
let
    cfg = config.bingux.nix;
in
{
    options.bingux.nix.allowUnfree = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Whether this profile may install packages marked as unfree.";
    };

    config = {
        nix = {
            settings = {
                experimental-features = [
                    "nix-command"
                    "flakes"
                ];
                auto-optimise-store = true;
                builders-use-substitutes = true;
                cores = 0;
                max-jobs = "auto";
                trusted-users = [
                    "root"
                    "@wheel"
                ];
            };

            gc = {
                automatic = true;
                dates = "weekly";
                options = "--delete-older-than 21d";
            };

            optimise.automatic = true;
        };

        nixpkgs.config.allowUnfree = cfg.allowUnfree;
    };
}
