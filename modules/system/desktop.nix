{
    config,
    lib,
    pkgs,
    ...
}:
let
    cfg = config.bingux.desktop;
    supports32BitGraphics = pkgs.stdenv.hostPlatform.isx86_64;
in
{
    options.bingux.desktop = {
        enable = lib.mkEnableOption "desktop services";

        graphics = {
            enable = lib.mkOption {
                type = lib.types.bool;
                default = cfg.enable;
                description = "Enable the generic graphics driver stack.";
            };

            enable32Bit = lib.mkOption {
                type = lib.types.bool;
                default = false;
                description = "Enable 32-bit graphics support on x86_64 systems.";
            };
        };

        audio = {
            enable = lib.mkOption {
                type = lib.types.bool;
                default = cfg.enable;
                description = "Enable PipeWire with PulseAudio emulation and WirePlumber.";
            };

            enable32Bit = lib.mkOption {
                type = lib.types.bool;
                default = false;
                description = "Enable 32-bit ALSA support for PipeWire.";
            };
        };
    };

    config = lib.mkMerge [
        {
            assertions = [
                {
                    assertion = !cfg.graphics.enable32Bit || supports32BitGraphics;
                    message = "bingux.desktop.graphics.enable32Bit requires an x86_64 system.";
                }
                {
                    assertion = !cfg.graphics.enable32Bit || cfg.graphics.enable;
                    message = "bingux.desktop.graphics.enable32Bit requires bingux.desktop.graphics.enable.";
                }
                {
                    assertion = !cfg.audio.enable32Bit || cfg.audio.enable;
                    message = "bingux.desktop.audio.enable32Bit requires bingux.desktop.audio.enable.";
                }
            ];
        }

        (lib.mkIf cfg.graphics.enable {
            hardware.graphics.enable = true;
        })

        (lib.mkIf (cfg.graphics.enable && cfg.graphics.enable32Bit && supports32BitGraphics) {
            hardware.graphics.enable32Bit = true;
        })

        (lib.mkIf cfg.audio.enable {
            security.rtkit.enable = true;

            services.pipewire = {
                enable = true;
                audio.enable = true;
                alsa = {
                    enable = true;
                    support32Bit = cfg.audio.enable32Bit;
                };
                pulse.enable = true;
                wireplumber.enable = true;
            };
        })
    ];
}
