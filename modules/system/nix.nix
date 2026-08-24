{ config, lib, ... }:
let
    cfg = config.bingux.nix;
in
{
    options.bingux.nix = {
        allowUnfree = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Whether this profile may install packages marked as unfree.";
        };

        buildCores = lib.mkOption {
            type = lib.types.ints.between 0 65536;
            default = 0;
            description = "The logical CPU count available to each Nix build. Zero uses all available CPUs.";
        };

        maxBuildJobs = lib.mkOption {
            type = lib.types.either (lib.types.ints.between 1 65536) (lib.types.enum [ "auto" ]);
            default = "auto";
            description = "The maximum number of concurrent Nix builds.";
        };
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
                cores = cfg.buildCores;
                max-jobs = cfg.maxBuildJobs;
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
