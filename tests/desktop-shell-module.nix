{
    inputs,
    self,
    system,
}:
let
    pkgs = inputs.nixpkgs.legacyPackages.${system};
    host = inputs.nixpkgs.lib.nixosSystem {
        inherit system;

        specialArgs = {
            hostSystem = system;
            inherit inputs self;
            hostName = "desktop-shell-check";
            profile = "desktop-shell-check";
        };

        modules = [
            inputs.home-manager.nixosModules.home-manager
            inputs.sops-nix.nixosModules.sops
            inputs.nix-flatpak.nixosModules.nix-flatpak
            self.nixosModules.default
            {
                bingux = {
                    desktop.enable = true;
                    desktop.gnoblin.enable = true;
                    desktopShell.enable = true;
                    desktopShell.metrics.enable = false;
                    desktopShell.dock.pinnedApps = [ "org.example.Terminal" ];
                    user.name = "shell";
                };

                home-manager.users.shell = {
                    home = {
                        username = "shell";
                        homeDirectory = "/home/shell";
                        stateVersion = "25.11";
                    };
                };
            }
        ];
    };
    quickshell = host.config.home-manager.users.shell.programs.quickshell;
    quickshellService = host.config.home-manager.users.shell.systemd.user.services.quickshell;
    shellConfig = host.config.home-manager.users.shell.xdg.configFile."quickshell/bingux";
    runtimeService = host.config.home-manager.users.shell.systemd.user.services.bingux-runtime-dir;
    statusService = host.config.home-manager.users.shell.systemd.user.services.bingux-statusd;
    searchService = host.config.home-manager.users.shell.systemd.user.services.bingux-searchd;
    shellSourceCheck = pkgs.runCommand "bingux-desktop-shell-source-check" { } ''
        for file in \
            shell.qml \
            Metrics.qml \
            Tray.qml \
            SystemIndicators.qml \
            Dock.qml \
            SearchOverlay.qml \
            SearchSocket.qml \
            InputSourceSelector.qml \
            PrivacyIndicators.qml \
            NotificationState.qml \
            NotificationSurface.qml \
            OsdState.qml \
            OsdSurface.qml \
            ProfileSettings.qml
        do
            test -f "${shellConfig.source}/$file"
        done
        grep -Fq "ProfileSettings {" "${shellConfig.source}/shell.qml"
        grep -Fq "Tray {" "${shellConfig.source}/shell.qml"
        grep -Fq "NotificationSurface {" "${shellConfig.source}/shell.qml"
        grep -Fq "OsdSurface {" "${shellConfig.source}/shell.qml"
        grep -Fq "settings: profileSettings" "${shellConfig.source}/shell.qml"
        grep -Fq "profileSettings.dockEnabled" "${shellConfig.source}/shell.qml"
        grep -Fq "profileSettings.metricsEnabled" "${shellConfig.source}/shell.qml"
        grep -Fq "gnoblinCtlPath: profileSettings.gnoblinCtlPath" "${shellConfig.source}/shell.qml"
        grep -Fq "timeoutPath: profileSettings.timeoutPath" "${shellConfig.source}/shell.qml"
        grep -Fq "acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton" "${shellConfig.source}/Tray.qml"
        grep -Fq "modelData.secondaryActivate()" "${shellConfig.source}/Tray.qml"
        grep -Fq "usesFallbackIcon" "${shellConfig.source}/Tray.qml"
        grep -Fq "TrayMenu" "${shellConfig.source}/Tray.qml"
        grep -Fq "maximumVisibleItems" "${shellConfig.source}/Tray.qml"
        grep -Fq "isSafeIconSource" "${shellConfig.source}/Tray.qml"
        grep -Fq "onWheel: function(wheel)" "${shellConfig.source}/Dock.qml"
        grep -Fq "Qt.MiddleButton" "${shellConfig.source}/Dock.qml"
        grep -Fq "Qt.RightButton" "${shellConfig.source}/Dock.qml"
        grep -Fq "currentGroup.desktopEntry.actions" "${shellConfig.source}/Dock.qml"
        grep -Fq "values: dockButton.currentGroup.windows" "${shellConfig.source}/Dock.qml"
        grep -Fq "onRead: function(data)" "${shellConfig.source}/SearchSocket.qml"
        grep -Fq "org.example.Terminal" "${shellConfig.source}/ProfileSettings.qml"
        grep -Fq "dockEnabled: true" "${shellConfig.source}/ProfileSettings.qml"
        grep -Fq "metricsEnabled: false" "${shellConfig.source}/ProfileSettings.qml"
        grep -Fq "timeoutPath" "${shellConfig.source}/ProfileSettings.qml"
        grep -Fq "gnoblinCtlPath" "${shellConfig.source}/ProfileSettings.qml"
        touch "$out"
    '';
in
assert quickshell.enable;
assert builtins.any (package: (package.pname or "") == "binguxctl") host.config.home-manager.users.shell.home.packages;
assert host.config.programs.dconf.enable;
assert host.config.bingux.desktop.gnoblin.settings.shell.osd == false;
assert !host.config.bingux.networking.tailscale.enable;
assert !(host.config.home-manager.users.shell.systemd.user.services ? bingux-tailscale-systray);
assert
    builtins.map (
        feature: feature.value
    ) host.config.home-manager.users.shell.dconf.settings."org/gnoblin/shell".disabled-features.value
    == [
        "notifications"
        "osd"
    ];
assert quickshell.activeConfig == "bingux";
assert quickshell.systemd.enable;
assert builtins.elem "LD_LIBRARY_PATH=" quickshellService.Service.Environment;
assert host.config.services.upower.enable;
assert runtimeService.Service.Type == "oneshot";
assert runtimeService.Service.RemainAfterExit;
assert runtimeService.Service.RuntimeDirectory == "bingux";
assert runtimeService.Service.RuntimeDirectoryMode == "0700";
assert runtimeService.Unit.PartOf == [ "graphical-session.target" ];
assert runtimeService.Install.WantedBy == [ "graphical-session.target" ];
assert !(statusService.Service ? RuntimeDirectory);
assert !(searchService.Service ? RuntimeDirectory);
assert
    statusService.Unit.After == [
        "graphical-session-pre.target"
        "bingux-runtime-dir.service"
    ];
assert
    searchService.Unit.After == [
        "graphical-session-pre.target"
        "bingux-runtime-dir.service"
    ];
assert statusService.Unit.Requires == [ "bingux-runtime-dir.service" ];
assert searchService.Unit.Requires == [ "bingux-runtime-dir.service" ];
assert
    searchService.Service.RestrictAddressFamilies == [
        "AF_UNIX"
        "AF_INET"
        "AF_INET6"
    ];
assert statusService.Install.WantedBy == [ "graphical-session.target" ];
assert searchService.Install.WantedBy == [ "graphical-session.target" ];
shellSourceCheck
