{ lib, stdenvNoCC, python3, glib, wrapGAppsHook3, makeWrapper }:
let python = python3.withPackages (ps: [ ps.pygobject3 ]);
in stdenvNoCC.mkDerivation {
    pname = "bingux-calendar";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ wrapGAppsHook3 makeWrapper ];
    buildInputs = [ glib ];
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${../../shell/bingux/calendar-events.py} "$out/libexec/calendar-events.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-calendar" \
            --add-flags "-u $out/libexec/calendar-events.py"
    '';
    meta = {
        description = "Read-only desktop calendar bridge for Bingux";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
        mainProgram = "bingux-calendar";
    };
}
