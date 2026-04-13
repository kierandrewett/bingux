{ lib, pkgs, ... }:
{
    programs.gnupg.agent = {
        enable = lib.mkDefault true;
        enableSSHSupport = lib.mkDefault true;
        pinentryPackage = lib.mkDefault pkgs.pinentry-gnome3;
    };
}
