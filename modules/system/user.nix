{
    config,
    lib,
    pkgs,
    ...
}:
let
    cfg = config.bingux.user;
in
{
    options.bingux.user = {
        name = lib.mkOption {
            type = lib.types.str;
            default = "bingux";
            description = "The login name owned by the selected profile.";
        };

        fullName = lib.mkOption {
            type = lib.types.str;
            default = "Bingux User";
            description = "The full name shown by user-facing tools.";
        };

        home = lib.mkOption {
            type = lib.types.str;
            default = "/home/${cfg.name}";
            description = "The home directory for the profile user.";
        };

        extraGroups = lib.mkOption {
            type = lib.types.listOf lib.types.str;
            default = [ ];
            description = "Additional system groups required by profile software.";
        };
    };

    config = {
        users.users.${cfg.name} = {
            isNormalUser = true;
            description = cfg.fullName;
            home = cfg.home;
            extraGroups = lib.unique (
                [
                    "audio"
                    "networkmanager"
                    "video"
                    "wheel"
                ]
                ++ cfg.extraGroups
            );
            shell = pkgs.zsh;
        };

        home-manager = {
            useGlobalPkgs = true;
            useUserPackages = true;
            backupFileExtension = "home-manager-backup";
        };
    };
}
