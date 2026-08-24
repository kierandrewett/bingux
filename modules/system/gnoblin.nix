{
    config,
    lib,
    ...
}:
let
    cfg = config.bingux.desktop.gnoblin;
in
{
    options.bingux.desktop.gnoblin.enable = lib.mkEnableOption "the Gnoblin Wayland session";

    config = lib.mkIf cfg.enable {
        assertions = [
            {
                assertion = config.bingux.desktop.enable;
                message = "bingux.desktop.gnoblin requires bingux.desktop.enable";
            }
        ];

        programs.gnoblin.enable = true;
    };
}
