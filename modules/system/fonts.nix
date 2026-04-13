{ config, lib, pkgs, ... }:
let
    cfg = config.bingux.fonts;

    google-sans-code = pkgs.runCommand "google-sans-code" { } ''
        mkdir -p $out/share/fonts/truetype
        cp ${../../files/fonts}/GoogleSansCode-*.ttf $out/share/fonts/truetype/
    '';

    plymouthFontPath = fontName:
        let
            fontMap = {
                "Adwaita Sans" = "${pkgs.adwaita-fonts}/share/fonts/Adwaita/AdwaitaSans-Regular.ttf";
                "Inter" = "${pkgs.inter}/share/fonts/truetype/InterVariable.ttf";
                "Cantarell" = "${pkgs.cantarell-fonts}/share/fonts/cantarell/Cantarell-VF.otf";
                "Roboto" = "${pkgs.roboto}/share/fonts/truetype/Roboto-Regular.ttf";
                "Noto Sans" = "${pkgs.noto-fonts}/share/fonts/noto/NotoSans-Regular.ttf";
            };
        in
        fontMap.${fontName} or "${pkgs.adwaita-fonts}/share/fonts/Adwaita/AdwaitaSans-Regular.ttf";
in
{
    options.bingux.fonts = {
        sansSerif = lib.mkOption {
            type = lib.types.str;
            default = "Adwaita Sans";
            description = "Default sans-serif font. Used in the DE, login screen, and Plymouth.";
            example = "Inter";
        };

        monospace = lib.mkOption {
            type = lib.types.str;
            default = "Google Sans Code";
            description = "Default monospace font. Used in terminals and code editors.";
            example = "JetBrains Mono";
        };

        serif = lib.mkOption {
            type = lib.types.str;
            default = "Noto Serif";
            description = "Default serif font.";
            example = "Source Serif";
        };
    };

    config = {
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
            sansSerif = lib.mkDefault [ cfg.sansSerif ];
            monospace = lib.mkDefault [ cfg.monospace ];
            serif = lib.mkDefault [ cfg.serif ];
        };

        # Plymouth uses the sans-serif font
        boot.plymouth.font = lib.mkDefault (plymouthFontPath cfg.sansSerif);
    };
}
