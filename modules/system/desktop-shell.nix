{
    config,
    lib,
    pkgs,
    ...
}:
let
    settingsBackend = pkgs.writeShellApplication {
        name = "bingux-settings-backend";
        runtimeInputs = [ pkgs.python3 pkgs.systemd pkgs.glib pkgs.imagemagick ];
        text = ''exec python3 ${../../shell/bingux/settings-backend.py} "$@"'';
    };
    filePreview = pkgs.callPackage ../../packages/bingux-file-preview { };
    desktopControls = pkgs.callPackage ../../packages/bingux-controls { };
    iconRenderer = pkgs.callPackage ../../packages/bingux-icon-renderer { };
    audioMeter = pkgs.callPackage ../../packages/bingux-audio-meter { };
    captureBackend = pkgs.callPackage ../../packages/bingux-capture { };
    binguxctl = pkgs.callPackage ../../packages/binguxctl { quickshell = cfg.package; configName = cfg.configName; };
    calendarBackend = pkgs.callPackage ../../packages/bingux-calendar { };
    caretBackend = pkgs.callPackage ../../packages/bingux-caret { };
    shellFiles = lib.cleanSourceWith {
        src = ../../shell/bingux;
        filter = path: type: builtins.baseNameOf path != "ProfileSettings.qml";
    };
    profileSettings = pkgs.writeText "bingux-${cfg.configName}-profile-settings.qml" ''
        import QtQuick

        QtObject {
            readonly property bool dockEnabled: ${lib.boolToString cfg.dock.enable} && BinguxPreferences.data.desktop.dock
            readonly property bool sidebarEnabled: ${lib.boolToString cfg.sidebar.enable} && BinguxPreferences.data.desktop.sidebar
            readonly property bool metricsEnabled: ${lib.boolToString cfg.metrics.enable} && BinguxPreferences.data.desktop.metrics
            readonly property var pinnedApps: ${builtins.toJSON cfg.dock.pinnedApps}
            readonly property string notificationDbusPath: "${lib.getExe' pkgs.glib "gdbus"}"
            readonly property string timeoutPath: "${lib.getExe' pkgs.coreutils "timeout"}"
            readonly property string gnoblinCtlPath: "${lib.getExe' config.programs.gnoblin.package "gnoblinctl"}"
        }
    '';
    shellEnvironment = [
        "QT_QPA_PLATFORM=wayland"
        # Gnoblin's session launcher exports its own Mutter lookup path. Do
        # not let that path make the Nix Qt libraries resolve against the
        # host Qt build; Quickshell and its QML modules must stay on one Qt
        # runtime or native reloads can crash.
        "LD_LIBRARY_PATH="
        "BINGUX_SETTINGS_HELPER=${lib.getExe settingsBackend}"
        "BINGUX_PREVIEW_HELPER=${filePreview}/bin/bingux-file-preview"
        "BINGUX_APP_LAUNCHER_HELPER=${desktopControls}/bin/bingux-launch-app"
        "BINGUX_CONTROLS_HELPER=${desktopControls}/bin/bingux-controls"
        "BINGUX_SESSION_INHIBIT=${pkgs.gnome-session}/bin/gnome-session-inhibit"
        "BINGUX_ICON_HELPER=${iconRenderer}/bin/bingux-icon-renderer"
        "BINGUX_AUDIO_METER=${audioMeter}/bin/bingux-audio-meter"
        "BINGUX_CAPTURE_HELPER=${lib.getExe captureBackend}"
        "BINGUX_CALENDAR_HELPER=${lib.getExe calendarBackend}"
        "BINGUX_CARET_HELPER=${lib.getExe caretBackend}"
        "QML_IMPORT_PATH=${pkgs.qt6.qtmultimedia}/lib/qt-6/qml:${textLayout}/lib/qt-6/qml${lib.optionalString cfg.sidebar.enable ":${terminalWidget}/lib/qt-6/qml"}"
    ];
    # QuickShell resolves the root QML file to its store directory before it
    # resolves local component types. Put the generated profile settings file
    # in that same immutable directory instead of relying on a separate home
    # configuration symlink.
    shellSource = pkgs.runCommand "bingux-${cfg.configName}-shell" { } ''
        mkdir -p "$out"
        cp -R ${shellFiles}/. "$out/"
        cp ${profileSettings} "$out/ProfileSettings.qml"
    '';
    settingsPlatform = pkgs.callPackage ../../packages/bingux-settings/platform { };
    settingsApp = pkgs.writeShellApplication {
        name = "bingux-settings";
        runtimeInputs = [ cfg.package ];
        text = ''
            # The desktop session may carry Gnoblin's Mutter library path.
            # Settings launches the Nix Quickshell package, so keep it on the
            # Qt runtime it was built with as the systemd shell services do.
            unset LD_LIBRARY_PATH
            export QML_IMPORT_PATH=${settingsPlatform}/lib/qt-6/qml
            export BINGUX_SETTINGS_QML=${shellSource}/settings.qml
            export BINGUX_QUICKSHELL=${lib.getExe cfg.package}
            export BINGUX_SETTINGS_HELPER=${lib.getExe settingsBackend}
            ${builtins.readFile ../../packages/bingux-settings/bingux-settings}
        '';
    };
    statusdPackage = pkgs.callPackage ../../packages/bingux-statusd { };
    searchdPackage = pkgs.callPackage ../../packages/bingux-searchd { };
    terminalWidget = pkgs.callPackage ../../packages/bingux-qmltermwidget { };
    textLayout = pkgs.callPackage ../../packages/bingux-text-layout { };
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
            default = pkgs.callPackage ../../packages/bingux-quickshell {};
            defaultText = lib.literalExpression "pkgs.callPackage ../../packages/bingux-quickshell {}";
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

        capture.shortcut = lib.mkOption {
            type = lib.types.str;
            default = "<Alt>s";
            description = "Global shortcut to open capture, or stop an active recording.";
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

        sidebar.enable = lib.mkOption {
            type = lib.types.bool;
            default = true;
            description = "Enable the edge-hover button and persistent terminal sidebar.";
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
        bingux.desktop.gnoblin.settings = {
            shell.osd = false;
            window-rules = lib.mkAfter [ {
                # Clip the client and draw both borders with the same shape.
                match.type = "window";
                corners = { radius = 14; smoothing = 0.0; mode = "force"; shadow-animation = { duration = 180; easing = "ease-out-cubic"; }; shadow = false; };
                # Applications and the X11 frame already provide their outline.
                # Keep this compositor border disabled to avoid stacked rings.
                borders = {
                    inner-width = 0;
                    inner-color = "#505050bf";
                    # Both compositor outline modes stay disabled here.
                    outer-width = 0;
                    outer-color = "#00000000";
                    radius = 14;
                    smoothing = 0.0;
                };
            } {
                match.type = "window";
                match.focused = true;
                corners.shadow = false;
            } {
                match.layer = "^(bingux-osd|bingux-snap-preview)$";
                animation = "none";
                opacity = 1.0;
                blur = 0;
            } {
                # Bingux keeps popup entry motion; the compositor owns the exit.
                match.layer = "^gnoblin-shell-popup$";
                animation = { "in" = "none"; out = "fade"; duration = 120; easing = "ease-out-cubic"; };
            } {
                match.layer = "^bingux-search$";
                animation = { "in" = "none"; out = "fade"; duration = 65; easing = "ease-out-cubic"; };
            } {
                # These surfaces are positioned or animated by Bingux itself.
                match.layer = "^(bingux-bar-tooltip|gnoblin-dock-tooltip|bingux-popup-dismiss|gnoblin-dock-launch|bingux-notifications)$";
                animation = "none";
            } {
                # Handles must map at the sidebar boundary, not slide in from the monitor edge.
                match.layer = "^bingux-sidebar-(edge|button|grip|corner)$";
                animation = "none";
            } {
                # QML owns panel colours; the compositor still blurs the backdrop.
                match.layer = "^(bingux-top-bar|bingux-terminal-sidebar|bingux-sidebar-corner|bingux-panel-outline|bingux-dock|bingux-search|gnoblin-shell-popup|bingux-bar-tooltip|gnoblin-dock-tooltip|bingux-notifications|bingux-capture-feedback)$";
                blur-ignore-shadows = true;
                opacity = 1.0;
                blur = 24;
            } ];
            shell.input-source-switcher = false;
            keybindings.wm.switch-input-source = [ ];
            keybindings.wm.switch-input-source-backward = [ ];
            keybindings.shell.show-screenshot-ui = [ ];
            shortcuts = lib.mkAfter [
                { name = "search"; binding = "Super"; capture-input = true; command = [ "${binguxctl}/bin/binguxctl" "search" "toggle" ]; }
                { name = "emoji"; binding = "<Super>period"; command = [ "${binguxctl}/bin/binguxctl" "emoji" "open" ]; }
                { name = "capture"; binding = cfg.capture.shortcut; command = [ "${binguxctl}/bin/binguxctl" "capture" "toggle" ]; }
            ];
        };

        home-manager.users.${config.bingux.user.name} = {
            home.packages = [ binguxctl settingsApp ];
            xdg.desktopEntries.bingux-settings = {
                name = "Bingux Settings";
                comment = "Search, AI, previews and desktop preferences";
                exec = "${settingsApp}/bin/bingux-settings";
                icon = "preferences-system";
                categories = [ "Settings" "DesktopSettings" ];
                terminal = false;
            };
            programs.quickshell = {
                enable = true;
                package = cfg.package;
                activeConfig = cfg.configName;
                systemd = {
                    enable = true;
                    target = cfg.systemdTarget;
                };
            };

            # Applications can request notifications before the shell starts.
            # Activate Bingux instead of an installed fallback such as Mako.
            xdg.dataFile."dbus-1/services/org.freedesktop.Notifications.service".text = ''
                [D-BUS Service]
                Name=org.freedesktop.Notifications
                Exec=${lib.getExe cfg.package} -c ${cfg.configName}
                SystemdService=quickshell.service
            '';

            dconf.settings."org/gnoblin/shell".disabled-features = [
                "notifications"
                "osd"
            ];

            # Bingux owns app switching. Release GNOME's built-in shortcuts.
            # Win+Period belongs to the visual Bingux emoji picker, not IBus's
            # inline emoji preedit. Keep IBus's alternate shortcut available.
            dconf.settings."org/freedesktop/ibus/panel/emoji".hotkey = [ "<Super>semicolon" ];
            dconf.settings."org/gnome/desktop/wm/keybindings" = {
                switch-applications = lib.gvariant.mkEmptyArray lib.gvariant.type.string;
                switch-applications-backward = lib.gvariant.mkEmptyArray lib.gvariant.type.string;
                switch-windows = lib.gvariant.mkEmptyArray lib.gvariant.type.string;
                switch-windows-backward = lib.gvariant.mkEmptyArray lib.gvariant.type.string;
            };
            xdg.configFile."gnoblin/scripts/compositor-bridge.js".source =
                "${config.programs.gnoblin.package}/share/gnoblin/scripts/compositor-bridge.js";
            xdg.configFile."gnoblin/scripts/lib".source =
                "${config.programs.gnoblin.package}/share/gnoblin/scripts/lib";
            xdg.configFile."gnoblin/scripts/bingux-osd.js".source = "${shellSource}/osd-bridge.js";
            xdg.configFile."bingux/switcher.json".text = builtins.toJSON {
                enabled = true;
                showDelay = 80;
            };

            xdg.configFile."quickshell/${cfg.configName}" = {
                source = shellSource;
                recursive = true;
            };

            # Statusd owns the OSD bridge as well as metric collection. Keep it
            # active when the top-bar metric display is disabled, otherwise Gnoblin
            # native OSD is disabled without a Bingux replacement.

            # Both daemons use the same runtime directory. A dedicated unit
            # owns its lifecycle so a daemon restart cannot remove the other
            # daemon's socket.
            systemd.user.services = {
                quickshell.Unit.Wants = [ "bingux-search-ui.service" "bingux-switcher-ui.service" ];
                quickshell.Service.Type = lib.mkForce "dbus";
                quickshell.Service.BusName = "org.freedesktop.Notifications";
                quickshell.Service.Restart = lib.mkForce "always";
                quickshell.Service.LimitCORE = 0;
                # User-systemd does not always import the session's Qt platform
                # selection before graphical-session.target. Select Wayland
                # explicitly so Qt does not attempt an unavailable X11 backend.
                quickshell.Service.Environment = shellEnvironment;

                bingux-search-ui = {
                    Unit = { Description = "Bingux search popout"; PartOf = [ cfg.systemdTarget ]; };
                    Service = {
                        ExecStart = "${lib.getExe cfg.package} --path ${shellSource}/SearchShell.qml --no-color";
                        Environment = shellEnvironment;
                        Restart = "always";
                        RestartSec = 1;
                        LimitCORE = 0;
                    };
                    Install.WantedBy = [ cfg.systemdTarget ];
                };
                bingux-switcher-ui = {
                    Unit = { Description = "Bingux window switcher"; PartOf = [ cfg.systemdTarget ]; };
                    Service = {
                        ExecStart = "${lib.getExe cfg.package} --path ${shellSource}/SwitcherShell.qml --no-color";
                        Environment = shellEnvironment;
                        Restart = "always";
                        RestartSec = 1;
                        LimitCORE = 0;
                    };
                    Install.WantedBy = [ cfg.systemdTarget ];
                };

                bingux-runtime-dir = {
                    Unit = {
                        Description = "Bingux desktop runtime directory";
                        PartOf = [ cfg.systemdTarget ];
                    };

                    Service = {
                        Type = "oneshot";
                        ExecStart = lib.getExe' pkgs.coreutils "true";
                        RemainAfterExit = true;
                        RuntimeDirectory = "bingux";
                        RuntimeDirectoryMode = "0700";
                    };

                    Install.WantedBy = [ cfg.systemdTarget ];
                };

                bingux-statusd = {
                    Unit = {
                        Description = "Bingux desktop-shell status and OSD bridge";
                        After = [
                            "graphical-session-pre.target"
                            "bingux-runtime-dir.service"
                        ];
                        Requires = [ "bingux-runtime-dir.service" ];
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
                        After = [
                            "graphical-session-pre.target"
                            "bingux-runtime-dir.service"
                        ];
                        Requires = [ "bingux-runtime-dir.service" ];
                        PartOf = [ cfg.systemdTarget ];
                    };

                    Service = {
                        ExecStart = "${lib.getExe cfg.search.package} --config ${searchConfig}";
                        NoNewPrivileges = true;
                        PrivateTmp = true;
                        ProtectHome = "read-only";
                        ProtectSystem = "strict";
                        ReadWritePaths = [ "%t/bingux" "-%h/.pi" "-%h/.claude" ];
                        Restart = "on-failure";
                        RestartSec = "1s";
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
