{ config, ... }:
let
    user = config.bingux.user;
in
{
    bingux = {
        profile.name = "kieran";

        system = {
            timeZone = "Europe/London";
            locale = "en_GB.UTF-8";
            keyMap = "uk";
        };

        user = {
            name = "kieran";
            fullName = "Kieran Drewett";
        };

        desktop = {
            enable = true;
            gnoblin.enable = true;
        };

        desktopShell.enable = true;

        performance = {
            enable = true;
            kernel = "cachyos-bore-lto-x86_64-v3";
            cpuGovernor = "performance";
            enableAmdPstate = true;
        };
    };

    services.displayManager = {
        gdm.enable = true;
        defaultSession = "gnoblin";
    };

    home-manager.users.${user.name} = {
        home = {
            username = user.name;
            homeDirectory = user.home;
            stateVersion = config.bingux.system.stateVersion;
        };

        programs.home-manager.enable = true;
    };
}
