{
    inputs,
    self,
    system,
}:
let
    host = inputs.nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = {
            inherit inputs self;
            hostName = "module-evaluation-check";
            profile = "none";
        };
        modules = [
            inputs.home-manager.nixosModules.home-manager
            inputs.sops-nix.nixosModules.sops
            inputs.nix-flatpak.nixosModules.nix-flatpak
            self.nixosModules.default
            {
                system.stateVersion = "25.11";
            }
        ];
    };
    pkgs = inputs.nixpkgs.legacyPackages.${system};
in
assert host.config.bingux.desktop.enable == false;
assert host.config.bingux.desktop.gnoblin.enable == false;
assert host.config.bingux.desktopShell.enable == false;
assert host.config.bingux.secrets.enable == false;
assert host.config.bingux.performance.enable == false;
pkgs.runCommand "bingux-module-evaluation-check" { } ''
    touch "$out"
''
