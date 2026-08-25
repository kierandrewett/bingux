{ inputs, self, ... }:
let
    system = "aarch64-linux";
    pkgs = inputs.nixpkgs.legacyPackages.${system};
    mkHost =
        hostSystem: modules:
        inputs.nixpkgs.lib.nixosSystem {
            system = hostSystem;
            specialArgs = {
                hostSystem = hostSystem;
                inherit inputs self;
                hostName = "bingux-performance-architecture-check";
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
            ]
            ++ modules;
        };
    genericHost = mkHost system [
        {
            bingux.performance.enable = true;
        }
    ];
    x86GenericHost = mkHost "x86_64-linux" [
        {
            bingux.performance.enable = true;
        }
    ];
    invalidKernel = builtins.tryEval (
        (mkHost system [
            {
                bingux.performance.kernel = "cachyos-bore-lto-x86_64-v3";
            }
        ]).config.bingux.performance.kernel
    );
in
assert genericHost.config.bingux.performance.kernel == "nixpkgs";
assert genericHost.config.hardware.cpu.amd.updateMicrocode == false;
assert !(builtins.elem "amd_pstate=active" genericHost.config.boot.kernelParams);
assert x86GenericHost.config.bingux.performance.kernel == "nixpkgs";
assert x86GenericHost.config.hardware.cpu.amd.updateMicrocode == true;
assert !(builtins.elem "amd_pstate=active" x86GenericHost.config.boot.kernelParams);
assert !invalidKernel.success;
pkgs.runCommand "bingux-performance-architecture-check" { } ''
    touch "$out"
''
