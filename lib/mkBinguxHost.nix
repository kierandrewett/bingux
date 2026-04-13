{ inputs, nixpkgs, nixpkgs-unstable, binguxModulesPath, binguxOverlays }:

{ hostname
, profile
, system ? "x86_64-linux"
, username ? "user"
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

        # Nixpkgs configuration with bingux overlays
        {
            nixpkgs = {
                config.allowUnfree = true;
                overlays = [
                    inputs.rust-overlay.overlays.default
                    inputs.nur.overlays.default
                    binguxOverlays
                ] ++ extraOverlays;
            };
        }
    ] ++ extraModules;
}
