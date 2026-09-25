import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const helperSource = readFileSync(new URL("../shell/bingux/SteamApplications.js", import.meta.url), "utf8");
const steamApplications = {};
vm.runInNewContext(helperSource, steamApplications, { filename: "SteamApplications.js" });

const game = { appId: "3830", name: "Happy Wheels", iconPath: "/steam/happy-wheels.png" };
const steamShortcut = {
    id: "steam_app_3830.desktop",
    name: "Happy Wheels",
    execString: "steam steam://rungameid/3830",
};
const nautilus = { id: "org.gnome.Nautilus.desktop", name: "Files", icon: "org.gnome.Nautilus" };

test("a known non-Steam app does not inherit a Steam match from its window title", () => {
    assert.equal(steamApplications.entryForWindow([steamShortcut], "org.gnome.Nautilus", "Happy Wheels", nautilus), null);
    assert.equal(steamApplications.gameForWindow([game], "org.gnome.Nautilus", "Happy Wheels", nautilus), null);
});

test("explicit Steam app IDs resolve independently of the window title", () => {
    assert.equal(steamApplications.gameForWindow([game], "steam_app_3830", "Browse local files").appId, "3830");
});

test("title fallback remains available for windows without a known desktop entry", () => {
    assert.equal(steamApplications.gameForWindow([game], "game-engine-window", "Happy Wheels").appId, "3830");
});
