{ config, lib, ... }:
let
    cfg = config.bingux;

    # Map locale prefixes to console keymaps
    keymapFor = locale:
        let
            prefix = builtins.head (builtins.split "\\." locale);
            country = builtins.head (builtins.match ".*_(.*)" prefix);
            map = {
                US = "us";
                GB = "uk";
                DE = "de";
                FR = "fr";
                ES = "es";
                IT = "it";
                PT = "pt-latin1";
                NL = "nl";
                SE = "sv-latin1";
                NO = "no";
                DK = "dk";
                FI = "fi";
                PL = "pl";
                CZ = "cz-lat2";
                RU = "ruwin_alt-UTF-8";
                JP = "jp106";
                KR = "us";
                BR = "br-abnt2";
                CA = "cf";
                AU = "us";
                NZ = "us";
                IN = "us";
            };
        in
        map.${country} or "us";

    localeToSupported = locale:
        let
            parts = builtins.split "\\." locale;
            lang = builtins.head parts;
            encoding = if builtins.length parts >= 3 then builtins.elemAt parts 2 else "UTF-8";
        in
        "${lang}.${encoding}/${encoding}";
in
{
    options.bingux = {
        locale = lib.mkOption {
            type = lib.types.str;
            default = "en_US.UTF-8";
            description = "Primary locale. Also sets the console keymap automatically.";
            example = "en_GB.UTF-8";
        };

        extraLocales = lib.mkOption {
            type = lib.types.listOf lib.types.str;
            default = [];
            description = "Additional locales to support alongside the primary locale.";
            example = [ "en_US.UTF-8" ];
        };
    };

    config = {
        i18n.defaultLocale = lib.mkDefault cfg.locale;
        console.keyMap = lib.mkDefault (keymapFor cfg.locale);

        i18n.supportedLocales = lib.mkDefault (
            lib.unique (
                [ (localeToSupported cfg.locale) ]
                ++ map localeToSupported cfg.extraLocales
            )
        );
    };
}
