{
    description = "Bingux: a profile-driven NixOS configuration framework";

    inputs = {
        nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

        home-manager = {
            url = "github:nix-community/home-manager";
            inputs.nixpkgs.follows = "nixpkgs";
        };

        sops-nix = {
            url = "github:Mic92/sops-nix";
            inputs.nixpkgs.follows = "nixpkgs";
        };

        nix-flatpak = {
            url = "github:gmodena/nix-flatpak/?ref=v0.7.0";
        };

        nix-cachyos-kernel.url = "github:xddxdd/nix-cachyos-kernel/release";

        nixos-generators = {
            url = "github:nix-community/nixos-generators";
            inputs.nixpkgs.follows = "nixpkgs";
        };

        gnoblin = {
            url = "github:kierandrewett/gnoblin";
            inputs.nixpkgs.follows = "nixpkgs";
        };
    };

    outputs =
        inputs@{
            self,
            nixpkgs,
            ...
        }:
        let
            systems = [
                "x86_64-linux"
                "aarch64-linux"
            ];
            forAllSystems = nixpkgs.lib.genAttrs systems;
            mkHost = import ./lib/mk-host.nix { inherit inputs self; };
        in
        {
            lib.mkHost = mkHost;

            nixosModules.default = {
                imports = [
                    inputs.gnoblin.nixosModules.default
                    ./modules
                ];
            };

            nixosConfigurations.bingux-vm = mkHost {
                system = "x86_64-linux";
                hostName = "bingux-vm";
                profile = "generic";
                modules = [ ./hosts/vm ];
            };

            nixosConfigurations.bingux-kieran-vm = mkHost {
                system = "x86_64-linux";
                hostName = "bingux-kieran-vm";
                profile = "kieran";
                modules = [ ./hosts/vm ];
            };

            packages = forAllSystems (
                system:
                let
                    pkgs = nixpkgs.legacyPackages.${system};
                in
                {
                    bingux-statusd = pkgs.callPackage ./packages/bingux-statusd { };
                }
            );

            checks = forAllSystems (
                system:
                {
                    desktop-shell-module = import ./tests/desktop-shell-module.nix {
                        inherit inputs self system;
                    };
                }
                // nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
                    kieran-profile = import ./tests/kieran-profile.nix {
                        inherit inputs self system;
                    };
                }
            );

            formatter = forAllSystems (
                system:
                let
                    pkgs = nixpkgs.legacyPackages.${system};
                in
                pkgs.writeShellApplication {
                    name = "bingux-format";
                    runtimeInputs = [
                        pkgs.git
                        pkgs.nixfmt
                    ];
                    text = ''
                        mapfile -d $'\0' files < <(git ls-files -z -- "*.nix")

                        if (( ''${#files[@]} > 0 )); then
                            nixfmt --indent=4 "''${files[@]}"
                        fi
                    '';
                }
            );
        };
}
