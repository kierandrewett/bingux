{ lib, stdenvNoCC, makeWrapper, python3, quickshell, configName ? "bingux" }:
stdenvNoCC.mkDerivation {
    pname = "binguxctl";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ makeWrapper ];
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${./binguxctl.py} "$out/libexec/binguxctl.py"
        makeWrapper ${python3}/bin/python3 "$out/bin/binguxctl" \
            --add-flags "$out/libexec/binguxctl.py" \
            --set-default BINGUX_QUICKSHELL ${lib.escapeShellArg (lib.getExe quickshell)} \
            --set-default BINGUX_CONFIG_NAME ${lib.escapeShellArg configName}
    '';
    meta = {
        description = "Command-line controls for the Bingux desktop shell";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
        mainProgram = "binguxctl";
    };
}
