{ lib, modulesPath, ... }:
{
    image.modules.bingux-installer =
        {
            config,
            ...
        }:
        {
            imports = [
                "${modulesPath}/installer/cd-dvd/installation-cd-base.nix"
            ];

            image.fileName = "bingux-${config.bingux.profile.name}.iso";

            # CachyOS kernel sets do not provide a ZFS module that matches the
            # Nixpkgs ZFS userspace package. Do not build an unsupported installer.
            boot.supportedFilesystems.zfs = lib.mkIf (lib.hasPrefix "cachyos-" config.bingux.performance.kernel) (
                lib.mkForce false
            );

            # A live installer must not force-import a ZFS root pool.
            boot.zfs.forceImportRoot = false;
        };
}
