{
    lib,
    python3,
    writeShellApplication,
}:
writeShellApplication {
    name = "bingux-inventory";
    runtimeInputs = [ python3 ];
    text = ''
        exec ${python3}/bin/python3 ${./bingux-inventory.py} "$@"
    '';

    meta = {
        mainProgram = "bingux-inventory";
        description = "Export installed package metadata without private configuration";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
    };
}
