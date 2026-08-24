{
    inputs,
    self,
    system,
}:
let
    pkgs = inputs.nixpkgs.legacyPackages.${system};
    mkHost = import ../lib/mk-host.nix { inherit inputs self; };
    host = mkHost {
        inherit system;
        hostName = "kieran-profile-check";
        profile = "kieran";
    };
in
assert host.config.bingux.profile.name == "kieran";
assert host.config.bingux.desktop.enable;
assert host.config.bingux.desktop.gnoblin.enable;
assert host.config.bingux.desktopShell.enable;
assert host.config.programs.gnoblin.enable;
assert host.config.services.displayManager.defaultSession == "gnoblin";
assert host.config.home-manager.users.kieran.programs.quickshell.enable;
pkgs.runCommand "bingux-kieran-profile-check" { } ''
    touch "$out"
''
