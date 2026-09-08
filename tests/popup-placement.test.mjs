import {readFileSync} from "node:fs";
import vm from "node:vm";
import test from "node:test";
import assert from "node:assert/strict";
const context = vm.createContext({});
vm.runInContext(readFileSync(new URL("../shell/bingux/PopupPlacement.js", import.meta.url), "utf8"), context);
const screen = {x: 0, y: 0, width: 1920, height: 1080};
const place = (anchor, output = screen) => context.place(anchor, output, 440, 408, 8, 12);
test("maps Wayland surface-local coordinates through shadow-inclusive buffer", () => {
    const caret = context.desktopCaret({x: 131, y: 67, height: 19,
        surface: {x: 0, y: 0, width: 290, height: 123}},
        {buffer: {x: 1275, y: 626, width: 290, height: 123}});
    assert.equal(caret.x, 1406);
    assert.equal(caret.y, 693);
});
test("above caret without covering its line", () => {
    const p = place({x: 800, y: 700, height: 24});
    assert.equal(p.y + 408, 688);
});
test("below a caret near the top", () => {
    assert.equal(place({x: 800, y: 40, height: 24}).y, 76);
});
test("clamps right edge and translates negative monitor origins", () => {
    const p = place({x: -10, y: 700, height: 20}, {...screen, x: -1920});
    assert.equal(p.x, 1472);
    assert.equal(p.y, 280);
});
test("short screen uses side without covering caret", () => {
    const p = place({x: 500, y: 300, height: 20}, {...screen, height: 600});
    assert.equal(p.x, 512);
    assert.ok(p.y >= 8 && p.y + 408 <= 592);
});
