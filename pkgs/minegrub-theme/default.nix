{ fetchFromGitHub, stdenvNoCC }:
stdenvNoCC.mkDerivation {
    pname = "minegrub-theme";
    version = "unstable-2025-11-07";

    src = fetchFromGitHub {
        owner = "Lxtharia";
        repo = "minegrub-theme";
        rev = "2fa2012472fbfcfea17b82655dd27456fa507ee7";
        hash = "sha256-GvlAAIpM/iZtl/EtI+LTzEsQ2qlUkex9i4xRUZXmadM=";
    };

    installPhase = ''
        runHook preInstall
        mkdir -p "$out"
        cp -r "$src/minegrub"/* "$out"/
        runHook postInstall
    '';
}
