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
                        stateVersion = "26.05";
                    };
                };
            }
        ];
    };
    quickshell = host.config.home-manager.users.shell.programs.quickshell;
    shellConfig = host.config.home-manager.users.shell.xdg.configFile."quickshell/bingux";
    statusService = host.config.home-manager.users.shell.systemd.user.services.bingux-statusd;
    searchService = host.config.home-manager.users.shell.systemd.user.services.bingux-searchd;
    profileSettings =
        host.config.home-manager.users.shell.xdg.configFile."quickshell/bingux/ProfileSettings.qml";
in
assert quickshell.enable;
assert host.config.programs.dconf.enable;
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
assert builtins.pathExists "${toString shellConfig.source}/shell.qml";
assert builtins.pathExists "${toString shellConfig.source}/Metrics.qml";
assert builtins.pathExists "${toString shellConfig.source}/Tray.qml";
assert builtins.pathExists "${toString shellConfig.source}/SystemIndicators.qml";
assert builtins.pathExists "${toString shellConfig.source}/Dock.qml";
assert builtins.pathExists "${toString shellConfig.source}/InputSourceSelector.qml";
assert builtins.pathExists "${toString shellConfig.source}/PrivacyIndicators.qml";
assert builtins.pathExists "${toString shellConfig.source}/NotificationState.qml";
assert builtins.pathExists "${toString shellConfig.source}/NotificationSurface.qml";
assert builtins.pathExists "${toString shellConfig.source}/OsdState.qml";
assert builtins.pathExists "${toString shellConfig.source}/OsdSurface.qml";
assert builtins.pathExists (toString profileSettings.source);
assert
    builtins.match "(.|\n)*org.example.Terminal(.|\n)*" (builtins.readFile profileSettings.source)
    != null;
assert
    builtins.match "(.|\n)*metricsEnabled: false(.|\n)*" (builtins.readFile profileSettings.source)
    != null;
assert
    builtins.match "(.|\n)*import QtQuick(.|\n)*gnoblinCtlPath(.|\n)*" (
        builtins.readFile profileSettings.source
    ) != null;
assert host.config.services.upower.enable;
assert !(statusService.Service ? RuntimeDirectory);
assert !(searchService.Service ? RuntimeDirectory);
assert
    searchService.Service.RestrictAddressFamilies == [
        "AF_UNIX"
        "AF_INET"
        "AF_INET6"
    ];
assert statusService.Install.WantedBy == [ "graphical-session.target" ];
assert searchService.Install.WantedBy == [ "graphical-session.target" ];
pkgs.runCommand "bingux-desktop-shell-module-check" { } ''
    touch "$out"
''
