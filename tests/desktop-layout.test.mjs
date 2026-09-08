import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
const layout = vm.createContext({});
vm.runInContext(readFileSync(new URL("../shell/bingux/DesktopLayout.js", import.meta.url), "utf8"), layout);
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
    assert.equal(layout.move(original, "search", "sidebar", 0), original);
    original.sidebar = ["notes"];
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
