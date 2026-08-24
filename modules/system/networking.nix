{ config, lib, ... }:
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
    };

    config = lib.mkMerge [
        {
            networking.networkmanager.enable = lib.mkDefault cfg.networkManager.enable;
            networking.firewall.enable = lib.mkDefault cfg.firewall.enable;
        }

        (lib.mkIf cfg.resolved.enable {
            services.resolved.enable = lib.mkDefault true;
        })
    ];
}
