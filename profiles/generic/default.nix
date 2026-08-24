{ config, ... }:
let
    user = config.bingux.user;
in
{
    bingux = {
        profile.name = "generic";
        user = {
            name = "bingux";
            fullName = "Bingux User";
        };
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
