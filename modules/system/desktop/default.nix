{ config, lib, ... }:
{
    options.bingux.desktop = lib.mkOption {
        type = lib.types.nullOr (lib.types.enum [ "gnome" ]);
        default = "gnome";
        description = "Desktop environment. Set to null for headless/server.";
    };

    imports = [
        ./gnome.nix
    ];
}
