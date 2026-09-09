const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../shell/bingux/SnapLayouts.js'), 'utf8').replace('.pragma library', '');
const layouts = vm.runInNewContext(source + '\n({defaults, valid, target, contains})');
assert(layouts.valid(layouts.defaults()));
assert(!layouts.valid([{tiles: [{x: 0, y: 0, width: 2, height: 1}]}]));
assert(!layouts.valid([{tiles: [{x: NaN, y: 0, width: 1, height: 1}]}]));
for (const area of [{x: -1920, y: 32, width: 1920, height: 970}, {x: 1280, y: 400, width: 1501, height: 850}]) {
    for (const layout of layouts.defaults()) for (const tile of layout.tiles) {
        const rect = layouts.target(tile, area, 4, 8);
        assert(rect.x >= area.x && rect.y >= area.y);
        assert(rect.x + rect.width <= area.x + area.width);
        assert(rect.y + rect.height <= area.y + area.height);
    }
    const [left, right] = layouts.defaults()[0].tiles.map(tile => layouts.target(tile, area, 4, 8));
    assert.equal(right.x - left.x - left.width, 4);
}
assert(!layouts.contains({x: 0, y: 0, width: 20, height: 20}, 20, 10));
console.log('PASS: default layouts, invalid regions, negative monitor origins, fractional dimensions, gaps, and boundary hit tests');
