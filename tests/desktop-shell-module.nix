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
                        stateVersion = "25.11";
                    };
                };
            }
        ];
    };
    quickshell = host.config.home-manager.users.shell.programs.quickshell;
    shellConfig = host.config.home-manager.users.shell.xdg.configFile."quickshell/bingux";
    statusService = host.config.home-manager.users.shell.systemd.user.services.bingux-statusd;
    searchService = host.config.home-manager.users.shell.systemd.user.services.bingux-searchd;
    shellSourceCheck = pkgs.runCommand "bingux-desktop-shell-source-check" { } ''
        for file in \
            shell.qml \
            Metrics.qml \
            Tray.qml \
            SystemIndicators.qml \
            Dock.qml \
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
        grep -Fq "org.example.Terminal" "${shellConfig.source}/ProfileSettings.qml"
        grep -Fq "metricsEnabled: false" "${shellConfig.source}/ProfileSettings.qml"
        grep -Fq "gnoblinCtlPath" "${shellConfig.source}/ProfileSettings.qml"
        touch "$out"
    '';
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
assert host.config.services.upower.enable;
assert statusService.Service.RuntimeDirectory == "bingux";
assert searchService.Service.RuntimeDirectory == "bingux";
assert
    searchService.Service.RestrictAddressFamilies == [
        "AF_UNIX"
        "AF_INET"
        "AF_INET6"
    ];
assert statusService.Install.WantedBy == [ "graphical-session.target" ];
assert searchService.Install.WantedBy == [ "graphical-session.target" ];
shellSourceCheck
