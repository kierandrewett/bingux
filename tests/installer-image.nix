{
    inputs,
    self,
    system,
}:
let
    pkgs = inputs.nixpkgs.legacyPackages.${system};
    genericImage = self.packages.${system}.bingux-generic-install-iso;
    kieranImage = self.packages.${system}.bingux-kieran-install-iso;
in
assert genericImage.passthru.config.image.filePath == "iso/bingux-generic.iso";
assert kieranImage.passthru.config.image.filePath == "iso/bingux-kieran.iso";
assert !kieranImage.passthru.config.boot.supportedFilesystems.zfs;
assert !kieranImage.passthru.config.boot.zfs.forceImportRoot;
assert kieranImage.passthru.config.xdg.portal.enable;
assert builtins.elem pkgs.xdg-desktop-portal-gtk kieranImage.passthru.config.xdg.portal.extraPortals;
pkgs.runCommand "bingux-installer-image-check" { } ''
    touch "$out"
''
