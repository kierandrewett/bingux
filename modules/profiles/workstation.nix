{ lib, ... }:
{
    systemd.targets.sleep.enable = lib.mkDefault false;
    systemd.targets.suspend.enable = lib.mkDefault false;
    systemd.targets.hibernate.enable = lib.mkDefault false;
    systemd.targets.hybrid-sleep.enable = lib.mkDefault false;
}
