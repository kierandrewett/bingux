{ pkgs, ... }:
let
    google-sans-code = pkgs.runCommand "google-sans-code" { } ''
        mkdir -p $out/share/fonts/truetype
        cp ${../../files/fonts}/GoogleSansCode-*.ttf $out/share/fonts/truetype/
    '';
in
{
    fonts.packages = with pkgs; [
        google-sans-code
        adwaita-fonts
        inter
        nerd-fonts.jetbrains-mono
        noto-fonts
        noto-fonts-cjk-sans
        noto-fonts-emoji
        jetbrains-mono
        roboto
        roboto-mono
        roboto-slab
        open-sans
        lato
        source-sans
        source-code-pro
        source-serif
        fira-code
        fira-sans
        ubuntu-sans
        liberation_ttf
    ];

    fonts.fontconfig.defaultFonts = {
        sansSerif = [ "Adwaita Sans" "Cantarell" ];
        monospace = [ "Google Sans Code" "JetBrains Mono" ];
    };
}
