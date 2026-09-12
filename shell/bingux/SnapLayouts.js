.pragma library

function defaults() {
    return [
        {
            id: "Halves",
            tiles: [
                { x: 0, y: 0, width: 0.5, height: 1 },
                { x: 0.5, y: 0, width: 0.5, height: 1 },
            ],
        },
        {
            id: "Wide right",
            tiles: [
                { x: 0, y: 0, width: 1 / 3, height: 1 },
                { x: 1 / 3, y: 0, width: 2 / 3, height: 1 },
            ],
        },
        { id: "Thirds", tiles: [0, 1, 2].map((i) => ({ x: i / 3, y: 0, width: 1 / 3, height: 1 })) },
        {
            id: "Quarters",
            tiles: [0, 1, 2, 3].map((i) => ({ x: (i % 2) / 2, y: Math.floor(i / 2) / 2, width: 0.5, height: 0.5 })),
        },
        {
            id: "Focus",
            tiles: [
                { x: 0, y: 0, width: 0.25, height: 1 },
                { x: 0.25, y: 0, width: 0.5, height: 1 },
                { x: 0.75, y: 0, width: 0.25, height: 1 },
            ],
        },
    ];
}

function valid(layouts) {
    return (
        Array.isArray(layouts) &&
        layouts.length > 0 &&
        layouts.length <= 12 &&
        layouts.every(
            (layout) =>
                Array.isArray(layout.tiles) &&
                layout.tiles.length > 0 &&
                layout.tiles.length <= 12 &&
                layout.tiles.every(
                    (tile) =>
                        [tile.x, tile.y, tile.width, tile.height].every(Number.isFinite) &&
                        tile.x >= 0 &&
                        tile.y >= 0 &&
                        tile.width > 0 &&
                        tile.height > 0 &&
                        tile.x + tile.width <= 1.001 &&
                        tile.y + tile.height <= 1.001,
                ),
        )
    );
}

function target(tile, area, inner, outer) {
    const left = Math.round(area.x + tile.x * area.width + (tile.x < 0.001 ? outer : inner / 2));
    const top = Math.round(area.y + tile.y * area.height + (tile.y < 0.001 ? outer : inner / 2));
    const right = Math.round(
        area.x + Math.min(1, tile.x + tile.width) * area.width - (tile.x + tile.width > 0.999 ? outer : inner / 2),
    );
    const bottom = Math.round(
        area.y + Math.min(1, tile.y + tile.height) * area.height - (tile.y + tile.height > 0.999 ? outer : inner / 2),
    );
    return { x: left, y: top, width: Math.max(1, right - left), height: Math.max(1, bottom - top) };
}

function contains(rect, x, y) {
    return x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height;
}

// Physical monitor edges remain reachable beyond panel/dock exclusive zones;
// destinations use the work area. Publish every edge at drag start so a fast
// drop does not depend on a last-moment UI round trip.
function edgeRegions(monitor, area, inner, outer) {
    const band = Math.min(28, monitor.width / 8, monitor.height / 8);
    const corner = Math.min(120, monitor.width / 4, monitor.height / 4);
    const list = [];
    for (const right of [false, true])
        for (const bottom of [false, true]) {
            const tile = { x: right ? 0.5 : 0, y: bottom ? 0.5 : 0, width: 0.5, height: 0.5 };
            const region = {
                id: "edge:" + (bottom ? "bottom" : "top") + "-" + (right ? "right" : "left"),
                layout: -1,
                control: false,
                target: target(tile, area, inner, outer),
            };
            list.push(
                Object.assign({}, region, {
                    hit: {
                        x: monitor.x + (right ? monitor.width - band : 0),
                        y: monitor.y + (bottom ? monitor.height - corner : 0),
                        width: band,
                        height: corner,
                    },
                }),
            );
            list.push(
                Object.assign({}, region, {
                    hit: {
                        x: monitor.x + (right ? monitor.width - corner : 0),
                        y: monitor.y + (bottom ? monitor.height - band : 0),
                        width: corner,
                        height: band,
                    },
                }),
            );
        }
    for (const right of [false, true])
        list.push({
            id: "edge:" + (right ? "right" : "left"),
            layout: -1,
            control: false,
            hit: {
                x: monitor.x + (right ? monitor.width - band : 0),
                y: monitor.y + corner,
                width: band,
                height: monitor.height - corner * 2,
            },
            target: target({ x: right ? 0.5 : 0, y: 0, width: 0.5, height: 1 }, area, inner, outer),
        });
    list.push({
        id: "edge:top",
        layout: -1,
        control: false,
        maximize: true,
        hit: { x: monitor.x + corner, y: monitor.y, width: monitor.width - corner * 2, height: band },
        target: { x: area.x, y: area.y, width: area.width, height: area.height },
    });
    return list;
}
