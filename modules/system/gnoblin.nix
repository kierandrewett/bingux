{
    config,
    lib,
    pkgs,
    ...
}:
let
    cfg = config.bingux.desktop.gnoblin;
    toml = pkgs.formats.toml { };
in
{
    options.bingux.desktop.gnoblin.enable = lib.mkEnableOption "the Gnoblin Wayland session";
    options.bingux.desktop.gnoblin.settings = lib.mkOption {
        type = toml.type;
        default = { };
        description = "Declarative gnoblin.toml settings, including native command shortcuts.";
    };

    config = lib.mkIf cfg.enable {
        assertions = [
            {
                assertion = config.bingux.desktop.enable;
                message = "bingux.desktop.gnoblin requires bingux.desktop.enable";
            }
            {
                assertion = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
                message = "bingux.desktop.gnoblin currently supports x86_64-linux only";
            }
        ];

        programs.gnoblin.enable = true;
        home-manager.users.${config.bingux.user.name}.xdg.configFile."gnoblin/gnoblin.toml".source =
            toml.generate "gnoblin.toml" cfg.settings;
    };
}
