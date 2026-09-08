{ lib, stdenvNoCC, python3, glib, networkmanager, gsettings-desktop-schemas, gnome-settings-daemon, gnome-session, wrapGAppsHook3, makeWrapper }:
let
    python = python3.withPackages (ps: [ ps.pygobject3 ]);
in
stdenvNoCC.mkDerivation {
    pname = "bingux-controls";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ wrapGAppsHook3 makeWrapper ];
    buildInputs = [ glib gsettings-desktop-schemas gnome-settings-daemon ];
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${../../shell/bingux/control-centre-services.py} "$out/libexec/control-centre-services.py"
        cp ${../../shell/bingux/launch-application.py} "$out/libexec/launch-application.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-launch-app" \
            --add-flags "$out/libexec/launch-application.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-controls" \
            --add-flags "-u $out/libexec/control-centre-services.py" \
            --prefix PATH : ${lib.makeBinPath [ networkmanager gnome-session ]}
    '';
    meta = {
        description = "VPN and desktop controls for Bingux";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
    };
}
