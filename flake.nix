{
    description = "Bingux — a NixOS-based Linux distribution";

    inputs = {
        nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
        nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";

        rust-overlay = {
            url = "github:oxalica/rust-overlay";
            inputs.nixpkgs.follows = "nixpkgs";
        };

        nur.url = "github:nix-community/NUR";

        aide-src = {
            url = "github:kierandrewett/aide";
            flake = false;
        };

        headline-zsh = {
            url = "github:Moarram/headline";
            flake = false;
        };

        sound-theme-frealtek = {
            url = "github:kierandrewett/sound-theme-frealtek";
            flake = false;
        };
    };

    outputs = inputs@{ self, nixpkgs, nixpkgs-unstable, ... }:
        let
            lib = nixpkgs.lib;

            mkPkgs = system:
                import nixpkgs {
                    inherit system;
                    config.allowUnfree = true;
                    overlays = [
                        inputs.rust-overlay.overlays.default
                        inputs.nur.overlays.default
                        (import ./overlays/default.nix { inherit inputs; })
                    ];
                };

            mkInstallerIso =
                let
                    system = "x86_64-linux";
                    buildDate = if self ? lastModifiedDate
                                then builtins.substring 0 8 self.lastModifiedDate
                                else "19700101";
                    buildSha = if self ? rev
                               then builtins.substring 0 8 self.rev
                               else "dirty";
                in
                (lib.nixosSystem {
                    inherit system;
                    specialArgs = { inherit inputs; };
                    modules = [
                        ./modules/installer/live-iso.nix
                        {
                            nixpkgs.config.allowUnfree = true;
                            nixpkgs.overlays = [
                                inputs.rust-overlay.overlays.default
                                inputs.nur.overlays.default
                                (import ./overlays/default.nix { inherit inputs; })
                            ];
                        }
                        ({ lib, ... }: {
                            image.baseName = lib.mkForce "bingux-${buildDate}-${buildSha}-${system}";
                        })
                    ];
                }).config.system.build.isoImage;
        in
        {
            # NixOS module that consumer flakes import
            nixosModules.bingux = import ./modules/system/common.nix;

            # Overlays for consumer flakes
            overlays.default = import ./overlays/default.nix { inherit inputs; };

            # Helper function for consumer flakes to build hosts
            lib.mkBinguxHost = import ./lib/mkBinguxHost.nix {
                inherit inputs nixpkgs nixpkgs-unstable;
                binguxModulesPath = ./modules;
                binguxOverlays = import ./overlays/default.nix { inherit inputs; };
            };

            # Generic installer ISO (no target host baked in)
            packages.x86_64-linux.installer-iso = mkInstallerIso;

            # Formatter
            formatter.x86_64-linux = (mkPkgs "x86_64-linux").nixpkgs-fmt;
        };
}
