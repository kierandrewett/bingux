{ stdenv, lib, qt6 }:
stdenv.mkDerivation {
    pname = "bingux-text-layout";
    version = "0.1.0";
    src = ./.;
    nativeBuildInputs = [ qt6.qmake ];
    buildInputs = [ qt6.qtbase qt6.qtdeclarative ];
    dontWrapQtApps = true;
    qmakeFlags = [ "text-layout.pro" ];
    doCheck = true;
    checkPhase = ''
        export XDG_CACHE_HOME="$TMPDIR/cache"
        mkdir -p "$XDG_CACHE_HOME"
        qmake spacing-test.pro -o Makefile.tests
        make -f Makefile.tests
        QT_QPA_PLATFORM=offscreen ./spacing-test
    '';
    installPhase = ''
        mkdir -p "$out/lib/qt-6/qml/Bingux/Text"
        cp libbinguxtext.so qmldir "$out/lib/qt-6/qml/Bingux/Text/"
    '';
    meta = {
        description = "Native paragraph spacing for Bingux notes";
        license = lib.licenses.mit;
        platforms = lib.platforms.linux;
    };
}
