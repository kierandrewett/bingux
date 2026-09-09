.pragma library

function defaults() {
    return [
        {id: "Halves", tiles: [{x: 0, y: 0, width: .5, height: 1}, {x: .5, y: 0, width: .5, height: 1}]},
        {id: "Wide right", tiles: [{x: 0, y: 0, width: 1/3, height: 1}, {x: 1/3, y: 0, width: 2/3, height: 1}]},
        {id: "Thirds", tiles: [0, 1, 2].map(i => ({x: i/3, y: 0, width: 1/3, height: 1}))},
        {id: "Quarters", tiles: [0, 1, 2, 3].map(i => ({x: (i%2)/2, y: Math.floor(i/2)/2, width: .5, height: .5}))},
        {id: "Focus", tiles: [{x: 0, y: 0, width: .25, height: 1}, {x: .25, y: 0, width: .5, height: 1}, {x: .75, y: 0, width: .25, height: 1}]}
    ];
}

function valid(layouts) {
    return Array.isArray(layouts) && layouts.length > 0 && layouts.length <= 12 && layouts.every(layout =>
        Array.isArray(layout.tiles) && layout.tiles.length > 0 && layout.tiles.length <= 12 && layout.tiles.every(tile =>
            [tile.x, tile.y, tile.width, tile.height].every(Number.isFinite) && tile.x >= 0 && tile.y >= 0
            && tile.width > 0 && tile.height > 0 && tile.x + tile.width <= 1.001 && tile.y + tile.height <= 1.001));
}

function target(tile, area, inner, outer) {
    const left = Math.round(area.x + tile.x * area.width + (tile.x < .001 ? outer : inner / 2));
    const top = Math.round(area.y + tile.y * area.height + (tile.y < .001 ? outer : inner / 2));
    const right = Math.round(area.x + Math.min(1, tile.x + tile.width) * area.width - (tile.x + tile.width > .999 ? outer : inner / 2));
    const bottom = Math.round(area.y + Math.min(1, tile.y + tile.height) * area.height - (tile.y + tile.height > .999 ? outer : inner / 2));
    return {x: left, y: top, width: Math.max(1, right - left), height: Math.max(1, bottom - top)};
}

function contains(rect, x, y) { return x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height; }
