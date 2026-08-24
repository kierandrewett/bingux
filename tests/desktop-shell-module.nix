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
            ../modules
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
in
assert quickshell.enable;
assert quickshell.activeConfig == "bingux";
assert quickshell.systemd.enable;
assert builtins.pathExists "${toString shellConfig.source}/shell.qml";
pkgs.runCommand "bingux-desktop-shell-module-check" { } ''
    touch "$out"
''
