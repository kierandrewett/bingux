{
    inputs,
    self,
    system,
}:
let
    pkgs = inputs.nixpkgs.legacyPackages.${system};
    genericImage = self.packages.${system}.bingux-generic-install-iso;
    kieranImage = self.packages.${system}.bingux-kieran-install-iso;
    # Evaluate each derivation and its stable output-path metadata without
    # realizing the expensive ISO payload in this focused flake check.
    imageMetadata =
        image:
        builtins.deepSeq image.drvPath {
            config = image.passthru.config.image;
            filePath = image.passthru.filePath;
        };
    genericMetadata = imageMetadata genericImage;
    kieranMetadata = imageMetadata kieranImage;
in
assert genericMetadata.config.baseName == "bingux-generic";
assert genericMetadata.config.fileName == "bingux-generic.iso";
assert genericMetadata.config.filePath == "iso/bingux-generic.iso";
assert genericMetadata.filePath == genericMetadata.config.filePath;
assert kieranMetadata.config.baseName == "bingux-kieran";
assert kieranMetadata.config.fileName == "bingux-kieran.iso";
assert kieranMetadata.config.filePath == "iso/bingux-kieran.iso";
assert kieranMetadata.filePath == kieranMetadata.config.filePath;
assert !kieranImage.passthru.config.boot.supportedFilesystems.zfs;
assert !kieranImage.passthru.config.boot.zfs.forceImportRoot;
assert kieranImage.passthru.config.xdg.portal.enable;
assert builtins.elem pkgs.xdg-desktop-portal-gtk
    kieranImage.passthru.config.xdg.portal.extraPortals;
pkgs.runCommand "bingux-installer-image-check" { } ''
    touch "$out"
''
