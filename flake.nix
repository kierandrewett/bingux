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

        # Gnoblin pairs its pinned Mutter and GNOME Shell sources with this
        # matching Nixpkgs revision. It must not follow Bingux's rolling input.
        gnoblin.url = "github:kierandrewett/gnoblin";
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
            host = import ./lib/mk-host.nix { inherit inputs self; };
            inherit (host) mkHost mkHostModules;


            mkInstallerImage =
                {
                    hostName,
                    profile,
                }:
                (mkHost {
                    system = "x86_64-linux";
                    inherit hostName profile;
                    modules = [ ./hosts/iso ];
                }).config.system.build.images.bingux-installer;
        in
        {
            lib = {
                inherit mkHost mkHostModules;
            };

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
                    bingux-searchd = pkgs.callPackage ./packages/bingux-searchd { };
                    bingux-inventory = pkgs.callPackage ./packages/bingux-inventory { };
                }
                // nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
                    bingux-generic-install-iso = mkInstallerImage {
                        hostName = "bingux-install";
                        profile = "generic";
                    };

                    bingux-kieran-install-iso = mkInstallerImage {
                        hostName = "bingux-kieran-install";
                        profile = "kieran";
                    };
                }
            );


            checks = forAllSystems (
                system:
                nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
                    desktop-shell-module = import ./tests/desktop-shell-module.nix {
                        inherit inputs self system;
                    };
                    kieran-profile = import ./tests/kieran-profile.nix {
                        inherit inputs self system;
                    };
                    installer-image = import ./tests/installer-image.nix {
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
