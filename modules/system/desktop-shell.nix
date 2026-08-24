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
            readonly property var pinnedApps: ${builtins.toJSON cfg.dock.pinnedApps}
        }
    '';
    statusdPackage = pkgs.callPackage ../../packages/bingux-statusd { };
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
            enable = lib.mkEnableOption "the Bingux desktop-shell metrics service" // {
                default = true;
            };

            package = lib.mkOption {
                type = lib.types.package;
                default = statusdPackage;
                defaultText = lib.literalExpression "pkgs.callPackage ./packages/bingux-statusd { }";
                description = "The metrics service package that supplies CPU, memory, and network samples.";
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

            systemd.user.services = lib.optionalAttrs cfg.metrics.enable {
                bingux-statusd = {
                    Unit = {
                        Description = "Bingux desktop-shell metrics service";
                        After = [ "graphical-session-pre.target" ];
                        PartOf = [ cfg.systemdTarget ];
                    };

                    Service = {
                        ExecStart = lib.getExe cfg.metrics.package;
                        NoNewPrivileges = true;
                        PrivateTmp = true;
                        ProtectHome = "read-only";
                        ProtectSystem = "strict";
                        Restart = "on-failure";
                        RestartSec = "1s";
                        RestrictAddressFamilies = [ "AF_UNIX" ];
                        RuntimeDirectory = "bingux";
                        RuntimeDirectoryMode = "0700";
                        UMask = "0077";
                    };

                    Install.WantedBy = [ cfg.systemdTarget ];
                };
            };
        };
    };
}
