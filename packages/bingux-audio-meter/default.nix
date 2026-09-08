{ lib, stdenv, pkg-config, libpulseaudio }:
stdenv.mkDerivation {
    pname = "bingux-audio-meter";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ libpulseaudio ];
    buildPhase = ''
        $CC -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror main.c -o bingux-audio-meter $(pkg-config --cflags --libs libpulse)
    '';
    installPhase = ''
        install -Dm755 bingux-audio-meter "$out/bin/bingux-audio-meter"
    '';
    meta = {
        description = "Per-application audio peak detection for dock badges";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
        mainProgram = "bingux-audio-meter";
    };
}
