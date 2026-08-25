{ config, lib, ... }:
let
    cfg = config.bingux.system;
in
{
    options.bingux.profile.name = lib.mkOption {
        type = lib.types.str;
        default = "generic";
        description = "The selected profile identifier.";
    };

    options.bingux.system = {
        stateVersion = lib.mkOption {
            type = lib.types.str;
            default = "25.11";
            description = "The NixOS state version for a new Bingux installation.";
        };

        timeZone = lib.mkOption {
            type = lib.types.str;
            default = "Etc/UTC";
            description = "The system time zone. Profiles can override this without changing shared modules.";
        };

        locale = lib.mkOption {
            type = lib.types.str;
            default = "en_GB.UTF-8";
            description = "The default locale for the system and profile homes.";
        };

        keyMap = lib.mkOption {
            type = lib.types.str;
            default = "uk";
            description = "The console keymap before the graphical session starts.";
        };
    };

    config = {
        system.stateVersion = cfg.stateVersion;

        time.timeZone = cfg.timeZone;
        i18n.defaultLocale = cfg.locale;
        console.keyMap = cfg.keyMap;

        services.timesyncd.enable = lib.mkDefault true;
        services.fwupd.enable = true;

        hardware.enableRedistributableFirmware = true;
        security.polkit.enable = true;

        programs.zsh.enable = true;
    };
}
