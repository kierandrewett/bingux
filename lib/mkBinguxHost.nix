{ inputs, nixpkgs, nixpkgs-unstable, binguxModulesPath, binguxOverlays }:

{ hostname
, profile
, system ? "x86_64-linux"
, username ? "user"
, hardwareConfigPath ? "machines/${hostname}"
, extraModules ? []
, extraOverlays ? []
, specialArgs ? {}
}:

let
    lib = nixpkgs.lib;

    pkgsUnstable = import nixpkgs-unstable {
        inherit system;
        config.allowUnfree = true;
    };
in
lib.nixosSystem {
    inherit system;

    specialArgs = {
        inherit inputs hostname pkgsUnstable;
    } // specialArgs;

    modules = [
        # Bingux system modules
        (binguxModulesPath + "/system/common.nix")

        # Machine profile (workstation, laptop, generic)
        (binguxModulesPath + "/profiles/${profile}.nix")

        # Expose hardwareConfigPath for the installer
        {
            options.bingux.hardwareConfigPath = lib.mkOption {
                type = lib.types.str;
                default = hardwareConfigPath;
                readOnly = true;
                description = "Path (relative to repo root) where hardware-configuration.nix is placed by the installer.";
            };
        }

        # Nixpkgs configuration with bingux overlays
        {
            nixpkgs = {
                config.allowUnfree = true;
                overlays = [
                    inputs.nur.overlays.default
                    binguxOverlays
                ] ++ extraOverlays;
            };
        }
    ] ++ extraModules;
}
