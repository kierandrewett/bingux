{ config, lib, ... }:
let
    cfg = config.bingux.packages;

    flatpakApp = {
        options = {
            appId = lib.mkOption {
                type = lib.types.str;
                description = "The fully qualified Flatpak application ID owned by this profile declaration.";
            };

            origin = lib.mkOption {
                type = lib.types.str;
                default = "flathub";
                description = "The Flatpak remote that provides the declared application; it defaults to flathub.";
            };
        };
    };

    flatpakRemote = {
        options = {
            name = lib.mkOption {
                type = lib.types.str;
                description = "The local Flatpak remote name used by an application origin.";
            };

            location = lib.mkOption {
                type = lib.types.str;
                description = "The Flatpak repository URL for this remote.";
            };
        };
    };
in
{
    options.bingux.packages = {
        system = lib.mkOption {
            type = lib.types.listOf lib.types.package;
            default = [ ];
            description = "System-owned package derivations installed through environment.systemPackages; remove a declaration to reverse it on the next system activation.";
        };

        user = lib.mkOption {
            type = lib.types.listOf lib.types.package;
            default = [ ];
            description = "Profile-owned package derivations installed in the selected user's Home Manager profile; removing a declaration reverses it on the next home activation.";
        };

        flatpaks = {
            enable = lib.mkOption {
                type = lib.types.bool;
                default = false;
                description = "Whether this profile may install the explicitly declared Flatpak applications; unmanaged applications remain untouched.";
            };

            remotes = lib.mkOption {
                type = lib.types.listOf (lib.types.submodule flatpakRemote);
                default = [ ];
                description = "Profile-owned Flatpak remotes. Declaring any remote replaces nix-flatpak's default remote list, so include flathub explicitly when it is required.";
            };

            apps = lib.mkOption {
                type = lib.types.listOf (lib.types.submodule flatpakApp);
                default = [ ];
                description = "Profile-owned Flatpak application declarations. Each entry has a required appId and an origin defaulting to flathub; removing an entry reverses its declarative installation without removing unmanaged applications.";
            };
        };
    };

    config = lib.mkMerge [
        {
            environment.systemPackages = cfg.system;
            home-manager.users.${config.bingux.user.name}.home.packages = cfg.user;

            services.flatpak.update.auto.enable = lib.mkDefault false;
            services.flatpak.update.onActivation = lib.mkDefault false;
            services.flatpak.uninstallUnmanaged = lib.mkDefault false;
        }

        (lib.mkIf cfg.flatpaks.enable {
            services.flatpak = {
                enable = true;
                packages = map (app: {
                    inherit (app) appId origin;
                }) cfg.flatpaks.apps;
            };
        })

        (lib.mkIf (cfg.flatpaks.enable && cfg.flatpaks.remotes != [ ]) {
            services.flatpak.remotes = cfg.flatpaks.remotes;
        })
    ];
}
