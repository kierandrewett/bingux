{
    glib,
    lib,
    makeWrapper,
    ripgrep,
    rustPlatform,
    wl-clipboard,
}:
rustPlatform.buildRustPackage {
    pname = "bingux-searchd";
    version = "0.1.0";

    src = lib.cleanSource ./.;

    cargoLock = {
        lockFile = ./Cargo.lock;
    };

    nativeBuildInputs = [ makeWrapper ];

    postFixup = ''
        wrapProgram "$out/bin/bingux-searchd" \
            --prefix PATH : ${
                lib.makeBinPath [
                    glib
                    ripgrep
                    wl-clipboard
                ]
            }
    '';

    meta = {
        mainProgram = "bingux-searchd";
        description = "Local provider host for the Bingux desktop search surface";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
    };
}
