{ config, lib, pkgs, ... }:
let
    v = config.system.nixos.version;
    bingux-icon = pkgs.runCommand "bingux-icon" { } ''
        dir="$out/share/icons/hicolor/256x256/apps"
        mkdir -p "$dir"
        cp ${../../files/branding/bingus.png} "$dir/bingux.png"
    '';
in
{
    environment.systemPackages = [
        bingux-icon
        (lib.hiPrio (pkgs.runCommand "nixos-icons-bingux" { } ''
            cp -r ${pkgs.nixos-icons} $out
            chmod -R u+w $out
            for dir in $out/share/icons/hicolor/*/apps; do
                cp ${../../files/branding/bingus.png} "$dir/nix-snowflake.png" 2>/dev/null || true
            done
        ''))
    ];

    environment.etc."bingus.ascii".source = ../../files/branding/bingus.ascii;
    environment.etc."bingus.png".source = ../../files/branding/bingus.png;
    environment.etc."bingus-fastfetch.png".source = ../../files/branding/bingus-fastfetch.png;

    # Use bingux fastfetch config system-wide
    environment.etc."xdg/fastfetch/config.jsonc".source = ../../files/fastfetch/config.jsonc;

    system.nixos.distroName = lib.mkDefault "Bingux";

    environment.etc."os-release".text = lib.mkForce ''
        NAME="Bingux"
        ID=bingux
        ID_LIKE=nixos
        VERSION="${v}"
        VERSION_CODENAME=bingus
        VERSION_ID="${v}"
        PRETTY_NAME="Bingux ${v} (Bingus)"
        LOGO=bingux
        HOME_URL="https://github.com/kierandrewett/bingux"
        DOCUMENTATION_URL="https://github.com/kierandrewett/bingux"
        BUG_REPORT_URL="https://github.com/kierandrewett/bingux/issues"
    '';

    environment.etc."issue".text = lib.mkForce ''
        \e[1;35mBingux ${v}\e[0m (\l)

    '';
}
