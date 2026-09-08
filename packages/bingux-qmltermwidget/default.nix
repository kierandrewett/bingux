{ stdenv, lib, fetchurl, qt6, pkg-config }:

stdenv.mkDerivation {
    pname = "bingux-qmltermwidget";
    version = "0.2.0-unstable-2026-05-31";
    src = fetchurl {
        name = "qmltermwidget-8913504.tar.gz";
        url = "https://codeload.github.com/Swordfish90/qmltermwidget/tar.gz/8913504fa2ebd220ebe7c680c32954e1b3c035c5";
        sha256 = "90d4ae5dbf063adfc7e0abb023ee00546fd717c0ec8a92d9b191b09df25a8d56";
    };
    patches = [ ./reflow.patch ./transparency.patch ./selection.patch ];
    nativeBuildInputs = [ qt6.qmake pkg-config ];
    buildInputs = [ qt6.qtbase qt6.qtdeclarative ];
    dontWrapQtApps = true;
    doCheck = true;
    checkPhase = ''
        runHook preCheck
        $CXX -std=c++17 -fPIC $(pkg-config --cflags Qt6Core Qt6Gui) -Ilib \
            ${./tests/reflow.cpp} -LQMLTermWidget -Wl,-rpath,"$PWD/QMLTermWidget" \
            -lqmltermwidget $(pkg-config --libs Qt6Core Qt6Gui) -o reflow-tests
        ./reflow-tests
        runHook postCheck
    '';
    installPhase = ''
        runHook preInstall
        mkdir -p "$out/lib/qt-6/qml"
        cp -r QMLTermWidget "$out/lib/qt-6/qml/"
        cp ${./Bingux.colorscheme} "$out/lib/qt-6/qml/QMLTermWidget/color-schemes/Bingux.colorscheme"
        runHook postInstall
    '';
    meta = {
        description = "Qt 6 terminal component for the Bingux Quickshell sidebar";
        homepage = "https://github.com/Swordfish90/qmltermwidget";
        license = lib.licenses.gpl2Plus;
        platforms = lib.platforms.linux;
    };
}
