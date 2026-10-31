pragma Singleton

import QtQuick
import Quickshell
import "BinguxIconAliases.js" as External
import "BinguxIconIndex.js" as Index

// Resolves an icon name to a file. Every symbolic name the shell asks for is a file of
// shell/bingux/icons (written by scripts/icons/build.py, listed in BinguxIconIndex.js);
// scripts/ui-lint.py fails when the shell asks for one that is not. Names that other
// programs send (gnome-settings-daemon's on-screen display, Bluetooth devices) go through
// BinguxIconAliases.js first. Only what no Bingux file covers is left to the system icon
// theme: the icons of other programs' applications, notifications and tray items.
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
            const canonical = External.names[candidate] || candidate;
            if (Index.names[canonical])
                return Qt.resolvedUrl("icons/" + canonical + ".svg");
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
