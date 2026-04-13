{ lib, stdenv, fetchurl, autoPatchelfHook, gcc }:
stdenv.mkDerivation {
    pname = "aide";
    version = "latest";

    src = fetchurl {
        url = "https://github.com/kierandrewett/aide/releases/latest/download/aide-x86_64-linux.tar.gz";
        hash = "sha256-lfB8Akp4MAcm1pQpUAkQ4BZudsCqSlsEKeCjJ/PUKEg=";
    };

    nativeBuildInputs = [ autoPatchelfHook ];
    buildInputs = [ gcc.cc.lib ];

    unpackPhase = ''
        mkdir -p src
        tar xzf $src -C src
    '';

    sourceRoot = "src";

    installPhase = ''
        runHook preInstall
        mkdir -p $out/bin
        cp aide aide-daemon $out/bin/
        chmod +x $out/bin/aide $out/bin/aide-daemon
        runHook postInstall
    '';

    meta = with lib; {
        description = "Terminal IDE with tabs, git integration, and file browser";
        homepage = "https://github.com/kierandrewett/aide";
        license = licenses.mit;
        mainProgram = "aide";
        platforms = [ "x86_64-linux" ];
    };
}
