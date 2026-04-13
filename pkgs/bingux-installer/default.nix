{ lib, stdenv, python3, gtk4, libadwaita, gobject-introspection,
  wrapGAppsHook4, glib, makeWrapper,
  gh, git, gparted, gptfdisk, util-linux, ssh-to-age,
  gnome-text-editor, cryptsetup, btrfs-progs, e2fsprogs,
  xfsprogs, dosfstools, nix, jq, coreutils }:

let
    pythonEnv = python3.withPackages (ps: [ ps.pygobject3 ]);
in
stdenv.mkDerivation {
    pname = "bingux-installer";
    version = "0.1.0";
    src = ../../installer;

    nativeBuildInputs = [ gobject-introspection wrapGAppsHook4 glib makeWrapper ];
    buildInputs = [ gtk4 libadwaita ];

    dontBuild = true;

    installPhase = ''
        runHook preInstall

        mkdir -p $out/lib/bingux-installer $out/share/bingux-installer
        cp -r src/* $out/lib/bingux-installer/
        cp ${../../files/branding/bingus.png} $out/share/bingux-installer/logo.png

        mkdir -p $out/bin
        makeWrapper ${pythonEnv}/bin/python3 $out/bin/bingux-installer \
            --add-flags "$out/lib/bingux-installer/main.py" \
            --set PYTHONPATH "$out/lib/bingux-installer"

        mkdir -p $out/share/applications
        cp data/*.desktop $out/share/applications/

        runHook postInstall
    '';

    preFixup = ''
        gappsWrapperArgs+=(
            --prefix PATH : ${lib.makeBinPath [
                gh git gparted gptfdisk util-linux ssh-to-age gnome-text-editor
                cryptsetup btrfs-progs e2fsprogs xfsprogs dosfstools nix jq coreutils
            ]}
        )
    '';

    meta = {
        description = "Bingux GTK4 graphical installer";
        license = lib.licenses.mit;
        platforms = lib.platforms.linux;
        mainProgram = "bingux-installer";
    };
}
