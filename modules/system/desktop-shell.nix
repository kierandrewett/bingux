{
    config,
    lib,
    pkgs,
    ...
}:
let
    shellSource = lib.cleanSourceWith {
        src = ../../shell/bingux;
        filter = path: type: builtins.baseNameOf path != "ProfileSettings.qml";
    };
    profileSettings = pkgs.writeText "bingux-${cfg.configName}-profile-settings.qml" ''
        import QtQuick

        QtObject {
            readonly property bool dockEnabled: ${lib.boolToString cfg.dock.enable}
            readonly property bool metricsEnabled: ${lib.boolToString cfg.metrics.enable}
            readonly property var pinnedApps: ${builtins.toJSON cfg.dock.pinnedApps}
            readonly property string gnoblinCtlPath: "${lib.getExe' config.programs.gnoblin.package "gnoblinctl"}"
        }
    '';
    statusdPackage = pkgs.callPackage ../../packages/bingux-statusd { };
    searchdPackage = pkgs.callPackage ../../packages/bingux-searchd { };
    searchConfig = pkgs.writeText "bingux-${cfg.configName}-search-v1.json" (
        builtins.toJSON {
            protocolVersion = 1;
            commands = cfg.search.commands;
            fileRoots = cfg.search.fileRoots;
            providerManifestPaths = map toString cfg.search.providerManifests;
            sqliteSources = map (source: {
                inherit (source)
                    id
                    displayName
                    databasePath
                    query
                    activationCommand
                    ;
            }) cfg.search.sqliteSources;
            weather = cfg.search.weather;
            ai = cfg.search.ai;
        }
    );
    cfg = config.bingux.desktopShell;
in
{
    options.bingux.desktopShell = {
        enable = lib.mkEnableOption "the Bingux Quickshell desktop shell";

        package = lib.mkOption {
            type = lib.types.package;
            default = pkgs.quickshell;
            defaultText = lib.literalExpression "pkgs.quickshell";
            description = "The pinned Quickshell package that runs the Bingux desktop shell.";
        };

        configName = lib.mkOption {
            type = lib.types.strMatching "[A-Za-z0-9_-]+";
            default = "bingux";
            description = "The named Quickshell configuration installed for the selected profile.";
        };

        systemdTarget = lib.mkOption {
            type = lib.types.str;
            default = "graphical-session.target";
            description = "The user-systemd target that owns the desktop-shell process.";
        };

        metrics = {
            enable = lib.mkEnableOption "the Bingux top-bar metrics display" // {
                default = true;
            };

            package = lib.mkOption {
                type = lib.types.package;
                default = statusdPackage;
                defaultText = lib.literalExpression "pkgs.callPackage ./packages/bingux-statusd { }";
                description = "The metrics service package that supplies CPU, memory, and network samples.";
            };
        };
        search = {
            enable = lib.mkEnableOption "the Bingux local search provider service" // {
                default = true;
            };

            package = lib.mkOption {
                type = lib.types.package;
                default = searchdPackage;
                defaultText = lib.literalExpression "pkgs.callPackage ./packages/bingux-searchd { }";
                description = "The package that owns the Bingux search socket and provider lifecycle.";
            };

            commands = {
                applicationLauncher = lib.mkOption {
                    type = lib.types.listOf lib.types.str;
                    default = [ (lib.getExe' pkgs.gtk3 "gtk-launch") ];
                    description = "Absolute argv used to launch a selected desktop entry; the desktop ID is appended.";
                };

                fileOpener = lib.mkOption {
                    type = lib.types.listOf lib.types.str;
                    default = [ (lib.getExe' pkgs.xdg-utils "xdg-open") ];
                    description = "Absolute argv used to open a selected file or directory; its path is appended.";
                };

                clipboard = lib.mkOption {
                    type = lib.types.listOf lib.types.str;
                    default = [ (lib.getExe' pkgs.wl-clipboard "wl-copy") ];
                    description = "Absolute argv used to copy a selected calculation result.";
                };
            };
            fileRoots = lib.mkOption {
                type = lib.types.listOf lib.types.str;
                default = [ config.bingux.user.home ];
                description = "Absolute directories that the background file index may read.";
            };

            providerManifests = lib.mkOption {
                type = lib.types.listOf lib.types.path;
                default = [ ];
                description = "Immutable profile-trusted search-provider manifest paths.";
            };

            sqliteSources = lib.mkOption {
                type = lib.types.listOf (
                    lib.types.submodule {
                        options = {
                            id = lib.mkOption {
                                type = lib.types.strMatching "[a-z0-9]+(-[a-z0-9]+)*";
                                description = "Stable identifier for this SQLite source.";
                            };

                            displayName = lib.mkOption {
                                type = lib.types.str;
                                description = "Human-readable SQLite source name.";
                            };

                            databasePath = lib.mkOption {
                                type = lib.types.str;
                                description = "Absolute path to a SQLite database opened read-only.";
                            };

                            query = lib.mkOption {
                                type = lib.types.str;
                                description = "Read-only SQL with ?1 for the query and ?2 for the result limit.";
                            };

                            activationCommand = lib.mkOption {
                                type = lib.types.listOf lib.types.str;
                                default = [ ];
                                description = "Trusted argv used for a selected result; {id} expands as one argument.";
                            };
                        };
                    }
                );
                default = [ ];
                description = "Profile-declared SQLite sources for parameterised search.";
            };

            weather = lib.mkOption {
                type = lib.types.nullOr (
                    lib.types.submodule {
                        options = {
                            latitude = lib.mkOption {
                                type = lib.types.float;
                                description = "Latitude for the profile's Open-Meteo weather cache.";
                            };

                            longitude = lib.mkOption {
                                type = lib.types.float;
                                description = "Longitude for the profile's Open-Meteo weather cache.";
                            };

                            refreshSeconds = lib.mkOption {
                                type = lib.types.ints.between 60 86400;
                                default = 900;
                                description = "Minimum interval between background weather-cache refreshes.";
                            };
                        };
                    }
                );
                default = null;
                description = "Optional Open-Meteo cache settings. A null value disables weather search.";
            };

            ai = lib.mkOption {
                type = lib.types.nullOr (
                    lib.types.submodule {
                        options = {
                            endpoint = lib.mkOption {
                                type = lib.types.str;
                                description = "HTTPS OpenAI-compatible chat-completions endpoint without credentials, query parameters, or fragments.";
                            };

                            model = lib.mkOption {
                                type = lib.types.str;
                                description = "Model name sent to the configured chat endpoint.";
                            };

                            apiKeyFile = lib.mkOption {
                                type = lib.types.str;
                                description = "Runtime-only path to an API key file, normally managed by SOPS-Nix.";
                            };
                        };
                    }
                );
                default = null;
                description = "Optional OpenAI-compatible quick-chat settings with a runtime secret file.";
            };
        };

        dock = {
            enable = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = "Enable the Bingux dock for the selected profile.";
            };

            pinnedApps = lib.mkOption {
                type = lib.types.listOf lib.types.str;
                default = [ ];
                example = [ "org.wezfurlong.wezterm" ];
                description = "Wayland application IDs or desktop-entry IDs that stay visible in the Bingux dock.";
            };
        };

    };

    config = lib.mkIf cfg.enable {
        assertions = [
            {
                assertion = config.bingux.desktop.enable;
                message = "bingux.desktopShell requires bingux.desktop.enable";
            }
            {
                assertion = config.bingux.desktop.gnoblin.enable;
                message = "bingux.desktopShell requires bingux.desktop.gnoblin.enable.";
            }
        ];

        programs.dconf.enable = true;

        home-manager.users.${config.bingux.user.name} = {
            programs.quickshell = {
                enable = true;
                package = cfg.package;
                activeConfig = cfg.configName;
                systemd = {
                    enable = true;
                    target = cfg.systemdTarget;
                };
            };

            dconf.settings."org/gnoblin/shell".disabled-features = [
                "notifications"
                "osd"
            ];

            xdg.configFile."quickshell/${cfg.configName}" = {
                source = shellSource;
                recursive = true;
            };

            xdg.configFile."quickshell/${cfg.configName}/ProfileSettings.qml".source = profileSettings;
            # Statusd owns the OSD bridge as well as metric collection. Keep it
            # active when the top-bar metric display is disabled, otherwise Gnoblin
            # native OSD is disabled without a Bingux replacement.


            systemd.user.services = {
                # User-systemd does not always import the session's Qt platform
                # selection before graphical-session.target. Select Wayland
                # explicitly so Qt does not attempt an unavailable X11 backend.
                quickshell.Service.Environment = [
                    "QT_QPA_PLATFORM=wayland"
                ];

                bingux-statusd = {
                    Unit = {
                        Description = "Bingux desktop-shell status and OSD bridge";
                        After = [ "graphical-session-pre.target" ];
                        PartOf = [ cfg.systemdTarget ];
                    };

                    Service = {
                        ExecStart = lib.getExe cfg.metrics.package;
                        NoNewPrivileges = true;
                        PrivateTmp = true;
                        ProtectHome = "read-only";
                        ProtectSystem = "strict";
                        ReadWritePaths = [ "%t/bingux" ];
                        Restart = "on-failure";
                        RestartSec = "1s";
                        RuntimeDirectory = "bingux";
                        RuntimeDirectoryMode = "0700";
                        RestrictAddressFamilies = [ "AF_UNIX" ];
                        UMask = "0077";
                    };

                    Install.WantedBy = [ cfg.systemdTarget ];
                };
            }
            // lib.optionalAttrs cfg.search.enable {
                bingux-searchd = {
                    Unit = {
                        Description = "Bingux desktop search provider service";
                        After = [ "graphical-session-pre.target" ];
                        PartOf = [ cfg.systemdTarget ];
                    };

                    Service = {
                        ExecStart = "${lib.getExe cfg.search.package} --config ${searchConfig}";
                        NoNewPrivileges = true;
                        PrivateTmp = true;
                        ProtectHome = "read-only";
                        ProtectSystem = "strict";
                        ReadWritePaths = [ "%t/bingux" ];
                        Restart = "on-failure";
                        RestartSec = "1s";
                        RuntimeDirectory = "bingux";
                        RuntimeDirectoryMode = "0700";
                        RestrictAddressFamilies = [
                            "AF_UNIX"
                            "AF_INET"
                            "AF_INET6"
                        ];
                        UMask = "0077";
                    };

                    Install.WantedBy = [ cfg.systemdTarget ];
                };
            };
        };
    };
}
