import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
const controls = vm.createContext({});
vm.runInContext(readFileSync(new URL("../shell/bingux/ControlLayout.js", import.meta.url), "utf8"), controls);
const layout = vm.createContext({ControlLayout: controls});
vm.runInContext(readFileSync(new URL("../shell/bingux/DesktopLayout.js", import.meta.url), "utf8").replace(/^\.import[^\n]+\n/, ""), layout);
const plain = value => JSON.parse(JSON.stringify(value));
test("moving widgets preserves other placements and never duplicates a widget", () => {
    const original = layout.defaults();
    const next = layout.move(original, "search", "dock", 0);
    assert.deepEqual(plain(next.dock), ["search"]);
    assert.deepEqual(plain(next["top-left"]), []);
    assert.deepEqual(plain(original["top-left"]), ["search"]);
    const reordered = layout.move(next, "notifications", "dock", 0);
    assert.deepEqual(plain(reordered.dock), ["notifications", "search"]);
    assert.equal(Object.values(reordered).flat().filter(id => id === "notifications").length, 1);
});
test("sidebar panels stay in compatible zones and the last panel cannot be removed", () => {
    const original = layout.defaults();
    assert.equal(layout.move(original, "notes", "dock", 0), original);
    assert.equal(layout.move(original, "search", "sidebar", 0).sidebar[0], "search");
    original.sidebar = ["search", "notes", "label:1"];
    assert.equal(layout.move(original, "notes", "palette", 0), original);
    assert.equal(layout.move(original, "unrecognised", "dock", 0), original);
});
test("palette removal and same-zone reordering preserve all remaining widgets", () => {
    let current = layout.move(layout.defaults(), "clock", "palette", 0);
    assert.equal(layout.zone(current, "clock"), "");
    current = layout.move(current, "notes", "sidebar", 0);
    assert.equal(current.sidebar[0], "notes");
    assert.equal(current.sidebar.length, 6);
});
test("presentation preserves native defaults and supports container inheritance and widget overrides", () => {
    const native = layout.presentation({}, "clock", "top-center", "12:30", "clock", false, true);
    assert.equal(native.custom, false);
    assert.equal(native.showIcon, false);
    assert.equal(native.showText, true);
    const desktop = {containers: {dock: {display: "text"}}, widgetOptions: {clock: {display: "inherit", label: "Time"}}};
    let face = layout.presentation(desktop, "clock", "dock", "12:30", "clock", false, true);
    assert.equal(face.label, "Time"); assert.equal(face.showIcon, false); assert.equal(face.showText, true);
    desktop.widgetOptions.clock = {display: "icons", icon: "starred-symbolic"};
    face = layout.presentation(desktop, "clock", "dock", "12:30", "clock", false, true);
    assert.equal(face.icon, "starred-symbolic"); assert.equal(face.showIcon, true); assert.equal(face.showText, false);
    desktop.widgetOptions.clock.display = "native";
    face = layout.presentation(desktop, "clock", "dock", "12:30", "clock", false, true);
    assert.equal(face.showIcon, false); assert.equal(face.showText, true);
});
test("drop positions follow visible neighbours in orders containing hidden widgets", () => {
    const order = ["hidden-first", "network", "vpn", "bluetooth", "hidden-last"];
    assert.equal(layout.insertionIndex(order, "network", ["bluetooth"], -1), 3);
    assert.equal(layout.insertionIndex(order, "network", ["bluetooth"], 0), 2);
    assert.equal(layout.insertionIndex(order, "new", ["network", "bluetooth"], 1), 3);
    assert.equal(layout.insertionIndex(order, "new", [], -1), 5);
    const moved = order.filter(id => id !== "network");
    moved.splice(layout.insertionIndex(order, "network", ["bluetooth"], -1), 0, "network");
    assert.deepEqual(moved, ["hidden-first", "vpn", "bluetooth", "network", "hidden-last"]);
});
test("control-centre widgets can use every bar and dock position without duplicates", () => {
    let current = layout.defaults();
    for (const target of ["dock", "top-left", "top-center", "top-right"]) {
        current = layout.move(current, "control-network", target, 0);
        assert.equal(layout.zone(current, "control-network"), target);
        assert.equal(Object.values(current).flat().filter(id => id === "control-network").length, 1);
        assert.deepEqual(plain(current.sidebar), plain(layout.defaults().sidebar));
    }
    current = layout.move(current, "control-network", "palette", 0);
    assert.equal(layout.zone(current, "control-network"), "");
});

test("space palette items create independent instances and springs stay in the bar", () => {
    let current = layout.defaults();
    current = layout.move(current, "spring", "top-left", 1);
    current = layout.move(current, "spring", "top-left", 2);
    current = layout.move(current, "spacer", "top-right", 0);
    assert.deepEqual(plain(current["top-left"]), ["search", "spring:1", "spring:2"]);
    assert.equal(current["top-right"][0], "spacer:1");
    const previous = current;
    assert.equal(layout.move(current, "spring:1", "dock", 0), previous);
    current = layout.move(current, "spring:1", "top-center", 0);
    assert.equal(current["top-center"][0], "spring:1");
    current = layout.move(current, "spring:2", "palette", 0);
    assert.equal(layout.zone(current, "spring:2"), "");
    assert.equal(layout.widget("spring:1").flexible, true);
    assert.equal(layout.widget("spring:unknown"), undefined);
});

test("labels and icons create independent movable instances", () => {
    let current = layout.defaults();
    current = layout.move(current, "label", "top-left", 0);
    current = layout.move(current, "label", "dock", 0);
    current = layout.move(current, "icon", "dock", 1);
    assert.deepEqual(plain(current.dock), ["label:2", "icon:1"]);
    assert.equal(current["top-left"][0], "label:1");
    current = layout.move(current, "label:1", "dock", 1);
    assert.deepEqual(plain(current.dock), ["label:2", "label:1", "icon:1"]);
    current = layout.move(current, "icon:1", "sidebar", 0);
    assert.equal(current.sidebar[0], "icon:1");
    assert.equal(layout.widget("label:0"), undefined);
    current = layout.move(current, "label:2", "palette", 0);
    assert.equal(layout.zone(current, "label:2"), "");
    assert.equal(layout.widget("icon:1").decoration, true);
});

test("native portable controls can move through bar and dock without duplicate placements", () => {
    for (const id of ["control-account", "control-settings", "control-session", "control-lock", "control-volume", "control-microphone", "control-battery", "control-media", "controls-header", "controls-audio", "controls-tiles", "control-divider", "control-header-space", "control-customise"]) {
        let current = layout.defaults();
        for (const zone of ["top-left", "top-center", "top-right", "dock", "palette"]) {
            assert.ok(layout.accepts(id, zone));
            current = layout.move(current, id, zone, 0);
            assert.equal(Object.values(current).flat().filter(value => value === id).length, zone === "palette" ? 0 : 1);
            assert.equal(layout.zone(current, id), zone === "palette" ? "" : zone);
        }
        assert.equal(layout.accepts(id, "sidebar"), false);
    }
});

test("group display inherits its host while child overrides stay independent", () => {
    const desktop = {containers: {dock: {display: "text"}}, widgetOptions: {}};
    const face = () => layout.presentation(desktop, "control-settings", "controls-header", "Settings", "settings", true, false, "dock");
    assert.equal(face().mode, "text");
    desktop.containers["controls-header"] = {display: "icons"};
    assert.equal(face().mode, "icons");
    desktop.widgetOptions["control-settings"] = {display: "both", label: "Preferences"};
    assert.equal(face().mode, "both"); assert.equal(face().label, "Preferences");
});

test('status and decoration widgets accept the control centre while panels keep their existing hosts', () => {
    assert.deepEqual(plain(vm.runInContext('widgets.filter(item => !item.panel).map(item => item.id).sort()', layout)),
        plain(vm.runInContext('externalIds.slice().sort()', controls)));
    for (const id of ['search', 'clock', 'controls', 'metrics', 'keyboard', 'tray', 'privacy', 'capture', 'overflow', 'notifications', 'label', 'label:1', 'icon:2'])
        assert.equal(layout.accepts(id, 'control-centre'), true);
    for (const id of ['terminal', 'spacer:1', 'spring:1', 'unknown'])
        assert.equal(layout.accepts(id, 'control-centre'), false);
});
