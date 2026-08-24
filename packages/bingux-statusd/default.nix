{
    lib,
    rustPlatform,
}:
rustPlatform.buildRustPackage {
    pname = "bingux-statusd";
    version = "0.1.0";

    src = lib.cleanSource ./.;

    cargoLock = {
        lockFile = ./Cargo.lock;
    };

    meta = {
        mainProgram = "bingux-statusd";
        description = "Low-overhead metrics feed for the Bingux desktop shell";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
    };
}
