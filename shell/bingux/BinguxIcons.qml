pragma Singleton

import QtQuick
import Quickshell
import "BinguxIconAliases.js" as External
import "BinguxIconIndex.js" as Index

// Resolves an icon name to a file. Every symbolic name the shell asks for is the shape name
// of a file of shell/bingux/icons (written by scripts/icons/build.py, listed in
// BinguxIconIndex.js); scripts/ui-lint.py fails when the shell asks for one that is not.
// Names that other programs send (gnome-settings-daemon's on-screen display, Bluetooth
// devices) and the names the set had before go through BinguxIconAliases.js first, which
// build.py writes from scripts/icons/aliases.tsv. Only what no Bingux file covers is left
// to the system icon theme: the icons of other programs' applications, notifications and
// tray items.
QtObject {
    // The shape name for `name` (as given, with or without -symbolic): an old or outside
    // name becomes the shape name of the icon that stands for it; any other name is kept.
    function canonical(name) {
        const text = String(name || "");
        if (text.endsWith("-symbolic"))
            return External.names[text] || text;
        const symbolic = External.names[text + "-symbolic"];
        return symbolic ? symbolic.replace(/-symbolic$/, "") : text;
    }

    function source(iconSource, fallback) {
        const original = String(iconSource || "");
        if (!original)
            return "";
        if (original.startsWith("qrc:/qs-blackhole"))
            return fallback === true ? "" : source("cube-symbolic");

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
            return source("cube-symbolic");
        }
        return themed;
    }
}
