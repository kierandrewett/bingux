{ lib, ... }:
{
    services.tlp.enable = lib.mkDefault true;
    services.power-profiles-daemon.enable = lib.mkForce false;
    services.thermald.enable = lib.mkDefault true;

    services.logind.lidSwitch = lib.mkDefault "suspend";
    services.logind.lidSwitchExternalPower = lib.mkDefault "ignore";

    powerManagement.powertop.enable = lib.mkDefault true;
}
