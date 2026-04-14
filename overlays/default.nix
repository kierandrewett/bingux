{ inputs }:
final: prev: {
    # Compat aliases — upstream flakes target nixpkgs-unstable where xorg.* was flattened
    libxtst = prev.xorg.libXtst;
    libxcb = prev.xorg.libxcb;
    libx11 = prev.xorg.libX11;
    libxcomposite = prev.xorg.libXcomposite;
    libxdamage = prev.xorg.libXdamage;
    libxext = prev.xorg.libXext;
    libxfixes = prev.xorg.libXfixes;
    libxrandr = prev.xorg.libXrandr;

    # Override nixos-icons to replace the NixOS snowflake with bingux logo in GDM
    nixos-icons = prev.runCommand "nixos-icons-bingux" { } ''
        cp -r ${prev.nixos-icons} $out
        chmod -R u+w $out
        for dir in $out/share/icons/hicolor/*/apps; do
            cp ${../files/branding/bingus.png} "$dir/nix-snowflake.png" 2>/dev/null || true
        done
    '';

    bingux-plymouth = final.callPackage ../pkgs/bingux-plymouth { };
os-helper = final.callPackage ../pkgs/os-helper { };
    bingux-installer = final.callPackage ../pkgs/bingux-installer { };
    bingux-cli = final.callPackage ../pkgs/bingux-cli { };
    fastfetch-wrapped = final.callPackage ../pkgs/fastfetch-wrapper { };
}
