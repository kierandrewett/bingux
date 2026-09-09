{ lib, stdenvNoCC, python3, gst_all_1, pipewire, gdk-pixbuf, wrapGAppsHook3, makeWrapper, grim, wl-clipboard, xdg-utils, pulseaudio, libcanberra-gtk3, sound-theme-freedesktop }:
let
    python = python3.withPackages (ps: [ ps.pygobject3 ]);
    plugins = with gst_all_1; [ gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav ];
in
stdenvNoCC.mkDerivation {
    pname = "bingux-capture";
    version = "0.1.0";
    dontUnpack = true;
    nativeBuildInputs = [ wrapGAppsHook3 makeWrapper ];
    buildInputs = [ gst_all_1.gstreamer pipewire gdk-pixbuf ] ++ plugins;
    installPhase = ''
        mkdir -p "$out/bin" "$out/libexec"
        cp ${../../shell/bingux/capture_service.py} "$out/libexec/capture_service.py"
        cp ${../../shell/bingux/capture_backend.py} "$out/libexec/capture_backend.py"
        cp ${../../shell/bingux/capture-notify.py} "$out/libexec/capture-notify.py"
        cp ${../../shell/bingux/capture-launch.py} "$out/libexec/capture-launch.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-capture-open" \
            --add-flags "$out/libexec/capture-launch.py"
        makeWrapper ${python}/bin/python3 "$out/bin/bingux-capture-backend" \
            --add-flags "-u $out/libexec/capture_service.py" \
            --prefix PATH : ${lib.makeBinPath [ grim wl-clipboard xdg-utils pulseaudio libcanberra-gtk3 ]} \
            --prefix XDG_DATA_DIRS : ${sound-theme-freedesktop}/share \
            --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : ${lib.makeSearchPath "lib/gstreamer-1.0" (plugins ++ [ pipewire ])}
    '';
    meta = {
        description = "PipeWire screenshots and compressed recordings with CPU H.264 fallback";
        license = lib.licenses.gpl3Plus;
        platforms = lib.platforms.linux;
        mainProgram = "bingux-capture-backend";
    };
}
