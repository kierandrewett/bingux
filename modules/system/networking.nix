{
    config,
    lib,
    pkgs,
    ...
}:
let
    cfg = config.bingux.networking;
in
{
    options.bingux.networking = {
        networkManager.enable = lib.mkOption {
            type = lib.types.bool;
            default = true;
            description = "Enable NetworkManager for general workstation networking.";
        };

        resolved.enable = lib.mkOption {
            type = lib.types.bool;
            default = cfg.networkManager.enable;
            description = "Enable systemd-resolved for DNS.";
        };

        firewall.enable = lib.mkOption {
            type = lib.types.bool;
            default = true;
            description = "Enable the NixOS stateful firewall.";
        };

        tailscale.enable = lib.mkEnableOption "the Tailscale daemon and StatusNotifierItem";
    };

    config = lib.mkMerge [
        {
            networking.networkmanager.enable = lib.mkDefault cfg.networkManager.enable;
            networking.firewall.enable = lib.mkDefault cfg.firewall.enable;
        }

        (lib.mkIf cfg.resolved.enable {
            services.resolved.enable = lib.mkDefault true;
        })

        (lib.mkIf cfg.tailscale.enable {
            services.tailscale.enable = true;

            # Tailscale documents `tailscale systray` as a normal user process.
            # Quickshell renders its StatusNotifierItem and delegates its menu.
            home-manager.users.${config.bingux.user.name}.systemd.user.services.bingux-tailscale-systray = {
                Unit = {
                    Description = "Tailscale StatusNotifierItem";
                    After = [ "graphical-session-pre.target" ];
                    PartOf = [ "graphical-session.target" ];
                };

                Service = {
                    ExecStart = "${lib.getExe pkgs.tailscale} systray";
                    NoNewPrivileges = true;
                    Restart = "on-failure";
                    RestartSec = "2s";
                };

                Install.WantedBy = [ "graphical-session.target" ];
            };
        })
    ];
}
