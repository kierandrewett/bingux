{ lib, stdenvNoCC, python3, gtk3, poppler_gi, libreoffice, ffmpeg, wrapGAppsHook3, makeWrapper }:
let
    python = python3.withPackages (ps: [ ps.pygobject3 ps.pycairo ps.pillow ps.markdown ps.pyyaml ]);
in
stdenvNoCC.mkDerivation {
    pname = "bingux-file-preview";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ wrapGAppsHook3 makeWrapper ];
    buildInputs = [ gtk3 poppler_gi ];
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${../../shell/bingux/preview-document.py} "$out/libexec/preview-document.py"
        cp ${../../shell/bingux/preview_extra.py} "$out/libexec/preview_extra.py"
        cp ${../../shell/bingux/preview_formats.py} "$out/libexec/preview_formats.py"
        cp ${../../shell/bingux/preview_service.py} "$out/libexec/preview_service.py"
        cp ${../../shell/bingux/preview_limits.py} "$out/libexec/preview_limits.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-file-preview" \
            --add-flags "$out/libexec/preview-document.py" \
            --prefix PATH : ${lib.makeBinPath [ libreoffice ffmpeg ]}
    '';
    meta = {
        description = "Local document, office, database and media previews for Bingux search";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
    };
}
