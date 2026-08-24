{ inputs, self }:
let
    mkHostModules =
        {
            hostName,
            profile,
            modules ? [ ],
            ...
        }:
        let
            profileModule = ../profiles + "/${profile}";
            profileImport =
                if builtins.pathExists profileModule then
                    profileModule
                else
                    throw "Bingux profile '${profile}' does not exist at ${toString profileModule}";
        in
        [
            inputs.home-manager.nixosModules.home-manager
            inputs.sops-nix.nixosModules.sops
            inputs.nix-flatpak.nixosModules.nix-flatpak
            self.nixosModules.default
            profileImport
            {
                networking.hostName = hostName;
            }
        ]
        ++ modules;

    mkHost =
        args@{
            system,
            hostName,
            profile,
            ...
        }:
        inputs.nixpkgs.lib.nixosSystem {
            inherit system;

            specialArgs = {
                inherit
                    inputs
                    self
                    hostName
                    profile
                    ;
            };

            modules = mkHostModules args;
        };
in
{
    inherit mkHost mkHostModules;
}
