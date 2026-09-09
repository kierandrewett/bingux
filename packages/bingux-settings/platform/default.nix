{ stdenv, lib, qt6 }:
stdenv.mkDerivation {
    pname = "bingux-settings-platform";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ qt6.qmake ];
    buildInputs = [ qt6.qtbase qt6.qtdeclarative ];
    dontWrapQtApps = true;
    qmakeFlags = [ "platform.pro" ];
    installPhase = ''
        mkdir -p "$out/lib/qt-6/qml/Bingux/Settings"
        cp libbinguxsettings.so qmldir "$out/lib/qt-6/qml/Bingux/Settings/"
    '';
    meta.license = lib.licenses.gpl3Plus;
}
