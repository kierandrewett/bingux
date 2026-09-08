{ lib, stdenvNoCC, python3, gtk3, librsvg, gnome-desktop, wrapGAppsHook3, makeWrapper }:
let
    python = python3.withPackages (ps: [ ps.pygobject3 ]);
in
stdenvNoCC.mkDerivation {
    pname = "bingux-icon-renderer";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ wrapGAppsHook3 makeWrapper ];
    buildInputs = [ gtk3 librsvg gnome-desktop ];
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${../../shell/bingux/render-os-icons.py} "$out/libexec/render-os-icons.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-icon-renderer" \
            --add-flags "-u $out/libexec/render-os-icons.py"
    '';
    meta = {
        description = "Cached OS icon rendering through GTK and librsvg";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
    };
}
