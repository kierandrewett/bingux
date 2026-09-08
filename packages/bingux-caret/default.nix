{ lib, stdenvNoCC, python3, at-spi2-core, wrapGAppsHook3, makeWrapper }:
let python = python3.withPackages (ps: [ ps.pygobject3 ]);
in stdenvNoCC.mkDerivation {
    pname = "bingux-caret";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ wrapGAppsHook3 makeWrapper ];
    buildInputs = [ at-spi2-core ];
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${../../shell/bingux/caret-anchor.py} "$out/libexec/caret-anchor.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-caret" \
            --add-flags "$out/libexec/caret-anchor.py"
    '';
    meta = {
        description = "Geometry-only accessibility caret query for Bingux";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
        mainProgram = "bingux-caret";
    };
}
