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
            self.nixosModules.default
            {
                bingux = {
                    desktop.enable = true;
                    desktopShell.enable = true;
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
in
assert quickshell.enable;
assert quickshell.activeConfig == "bingux";
assert quickshell.systemd.enable;
assert builtins.pathExists "${toString shellConfig.source}/shell.qml";
assert builtins.pathExists "${toString shellConfig.source}/Metrics.qml";
assert statusService.Service.RuntimeDirectory == "bingux";
assert statusService.Install.WantedBy == [ "graphical-session.target" ];
pkgs.runCommand "bingux-desktop-shell-module-check" { } ''
    touch "$out"
''
