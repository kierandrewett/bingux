pragma Singleton

import QtQuick
import Quickshell
import "AdwaitaIconIndex.js" as IconIndex

QtObject {
    function source(iconSource, fallback) {
        const original = String(iconSource || "");
        if (!original)
            return "";
        if (original.startsWith("qrc:/qs-blackhole"))
            return fallback === true ? "" : source("application-x-executable-symbolic");

        const imageThemePrefix = "image://icon/";
        let iconName = original;
        if (iconName.startsWith(imageThemePrefix))
            iconName = iconName.slice(imageThemePrefix.length);
        else if (iconName.startsWith("/") || /^[a-z][a-z0-9+.-]*:/i.test(iconName))
            return original;

        const fallbackMatch = /[?&]fallback=([^&]+)/.exec(iconName);
        const fallbackName = fallbackMatch ? decodeURIComponent(fallbackMatch[1]) : typeof fallback === "string" ? fallback : "";
        iconName = decodeURIComponent(iconName.split("?", 1)[0]);

        const candidates = [iconName, fallbackName];
        for (const candidate of candidates) {
            if (!candidate)
                continue;
            const canonical = IconIndex.aliases[candidate] || candidate;
            const relativePath = IconIndex.paths[canonical];
            if (!relativePath)
                continue;

            const themeDirectory = Quickshell.env("BINGUX_ADWAITA_ICON_DIR");
            if (themeDirectory)
                return "file://" + themeDirectory + "/symbolic/" + relativePath;
            return Qt.resolvedUrl("../../subprojects/adwaita-icon-theme/Adwaita/symbolic/" + relativePath);
        }

        const themed = original.startsWith(imageThemePrefix) ? original : fallback === undefined ? Quickshell.iconPath(original) : Quickshell.iconPath(original, fallback);
        if (String(themed).startsWith("qrc:/qs-blackhole")) {
            if (fallback === true)
                return "";
            return source("application-x-executable-symbolic");
        }
        return themed;
    }
}
