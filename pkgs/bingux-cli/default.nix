{ lib, stdenv, python3, makeWrapper }:
stdenv.mkDerivation {
    pname = "bingux-cli";
    version = "0.1.0";
    src = ./.;

    nativeBuildInputs = [ makeWrapper ];

    dontBuild = true;

    installPhase = ''
        runHook preInstall
        mkdir -p $out/lib/bgx $out/bin
        cp bgx.py $out/lib/bgx/bgx.py
        ${python3}/bin/python3 -m compileall -q $out/lib/bgx/
        makeWrapper ${python3}/bin/python3 $out/bin/bgx \
            --add-flags "$out/lib/bgx/bgx.py"
        runHook postInstall
    '';

    meta = {
        description = "Bingux package manager (bgx)";
        license = lib.licenses.mit;
        mainProgram = "bgx";
    };
}
