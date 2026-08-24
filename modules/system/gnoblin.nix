{
    config,
    lib,
    pkgs,
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
            {
                assertion = pkgs.stdenv.hostPlatform.system == "x86_64-linux";
                message = "bingux.desktop.gnoblin currently supports x86_64-linux only";
            }
        ];

        programs.gnoblin.enable = true;
    };
}
