{ lib, ... }:
{
    services.pulseaudio.enable = lib.mkDefault false;

    services.pipewire = {
        enable = lib.mkDefault true;
        alsa = {
            enable = lib.mkDefault true;
            support32Bit = lib.mkDefault true;
        };
        pulse.enable = lib.mkDefault true;
        jack.enable = lib.mkDefault true;
        wireplumber.extraConfig."10-default-clock" = {
            "wireplumber.settings" = {
                "default.clock.rate" = lib.mkDefault 48000;
                "default.clock.allowed-rates" = lib.mkDefault [ 44100 48000 96000 ];
                "default.clock.quantum" = lib.mkDefault 1024;
                "default.clock.min-quantum" = lib.mkDefault 512;
                "default.clock.max-quantum" = lib.mkDefault 2048;
            };
        };
    };

    security.rtkit.enable = lib.mkDefault true;
    programs.dconf.enable = lib.mkDefault true;
}
