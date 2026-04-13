{ lib, stdenv, python3, makeWrapper }:
stdenv.mkDerivation {
    pname = "bingux-cli";
    version = "0.2.0";
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

        # Zsh completions
        mkdir -p $out/share/zsh/site-functions
        cp _bgx $out/share/zsh/site-functions/_bgx

        runHook postInstall
    '';

    meta = {
        description = "Bingux package manager (bgx)";
        license = lib.licenses.mit;
        mainProgram = "bgx";
    };
}
