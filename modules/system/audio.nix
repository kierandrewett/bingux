{ ... }:
{
    services.pulseaudio.enable = false;

    services.pipewire = {
        enable = true;
        alsa = {
            enable = true;
            support32Bit = true;
        };
        pulse.enable = true;
        jack.enable = true;
        wireplumber.extraConfig."10-default-clock" = {
            "wireplumber.settings" = {
                "default.clock.rate" = 48000;
                "default.clock.allowed-rates" = [ 44100 48000 96000 ];
                "default.clock.quantum" = 1024;
                "default.clock.min-quantum" = 512;
                "default.clock.max-quantum" = 2048;
            };
        };
    };

    security.rtkit.enable = true;
    programs.dconf.enable = true;
}
