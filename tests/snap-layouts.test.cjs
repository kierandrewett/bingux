const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const path = require("node:path");
const source = fs
    .readFileSync(path.join(__dirname, "../shell/bingux/SnapLayouts.js"), "utf8")
    .replace(".pragma library", "");
const layouts = vm.runInNewContext(source + "\n({defaults, valid, target, contains, edgeRegions})");
assert(layouts.valid(layouts.defaults()));
assert(!layouts.valid([{ tiles: [{ x: 0, y: 0, width: 2, height: 1 }] }]));
assert(!layouts.valid([{ tiles: [{ x: NaN, y: 0, width: 1, height: 1 }] }]));
for (const area of [
    { x: -1920, y: 32, width: 1920, height: 970 },
    { x: 1280, y: 400, width: 1501, height: 850 },
]) {
    for (const layout of layouts.defaults())
        for (const tile of layout.tiles) {
            const rect = layouts.target(tile, area, 4, 8);
            assert(rect.x >= area.x && rect.y >= area.y);
            assert(rect.x + rect.width <= area.x + area.width);
            assert(rect.y + rect.height <= area.y + area.height);
        }
    const [left, right] = layouts.defaults()[0].tiles.map((tile) => layouts.target(tile, area, 4, 8));
    assert.equal(right.x - left.x - left.width, 4);
}
assert(!layouts.contains({ x: 0, y: 0, width: 20, height: 20 }, 20, 10));
for (const monitor of [
    { x: 0, y: 0, width: 1280, height: 800 },
    { x: -1501, y: 200, width: 1501, height: 901 },
]) {
    const area = { x: monitor.x, y: monitor.y + 32, width: monitor.width, height: monitor.height - 120 };
    const edges = layouts.edgeRegions(monitor, area, 8, 8);
    for (const [x, y, id] of [
        [0, 0.5, "left"],
        [0.999, 0.5, "right"],
        [0, 0, "top-left"],
        [0.999, 0, "top-right"],
        [0, 0.999, "bottom-left"],
        [0.999, 0.999, "bottom-right"],
    ]) {
        const hit = edges.find((r) =>
            layouts.contains(r.hit, monitor.x + x * monitor.width, monitor.y + y * monitor.height),
        );
        assert.equal(hit.id, "edge:" + id);
        assert(hit.target.x >= area.x && hit.target.y >= area.y);
        assert(hit.target.x + hit.target.width <= area.x + area.width);
        assert(hit.target.y + hit.target.height <= area.y + area.height);
    }
    const top = edges.find((r) => layouts.contains(r.hit, monitor.x + monitor.width / 2, monitor.y));
    assert(top, "Dropping at the top edge must snap");
    assert.equal(top.id, "edge:top");
    assert.equal(top.maximize, true);
    assert.equal(top.target.width, area.width);
    assert.equal(top.target.height, area.height);
    assert(
        !edges.some((r) => layouts.contains(r.hit, monitor.x + monitor.width / 2, monitor.y + monitor.height / 2)),
        "Centre does not snap",
    );
}
console.log(
    "PASS: default layouts, invalid regions, negative monitor origins, fractional dimensions, gaps, and boundary hit tests",
);
