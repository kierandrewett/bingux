{ ... }:
{
    services.tlp.enable = true;
    services.power-profiles-daemon.enable = false;
    services.thermald.enable = true;

    services.logind.lidSwitch = "suspend";
    services.logind.lidSwitchExternalPower = "ignore";

    powerManagement.powertop.enable = true;
}
