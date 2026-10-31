import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

// DockFolders.js is a QML library: strip its `.pragma` line and run it plain.
const folders = vm.createContext({});
vm.runInContext(
    readFileSync(new URL("../shell/bingux/DockFolders.js", import.meta.url), "utf8").replace(/^\.pragma[^\n]+\n/, ""),
    folders,
);
const plain = (value) => JSON.parse(JSON.stringify(value));
// The dock names an application by its group id (a window class such as
// "Firefox") or by its desktop id; both map to one identity.
const aliases = { Firefox: "firefox", "org.gnome.Nautilus.desktop": "org.gnome.Nautilus" };
const ctx = { identityOf: (id) => aliases[id] || id.replace(/\.desktop$/, "") };
const layout = (parts = {}) => ({ pinned: [], order: [], folders: [], ...parts });
const folder = (id, apps, extra = {}) => ({ id, name: "Folder", color: "blue", apps, ...extra });
const spec = (extra) => ({ id: "folder:a", source: "b", target: "a", visible: ["a", "b", "c"], ...extra });

test("dropping an app on another app makes a folder at the target's slot", () => {
    const start = layout({ pinned: ["a", "b", "c"], order: ["a", "b", "c"] });
    const next = folders.create(start, ctx, spec());
    assert.deepEqual(plain(next.folders), [folder("folder:a", ["a", "b"], { name: "Folder" })]);
    assert.deepEqual(plain(next.order), ["folder:a", "c"]);
    assert.deepEqual(
        plain(start),
        plain(layout({ pinned: ["a", "b", "c"], order: ["a", "b", "c"] })),
        "input is untouched",
    );
});

test("a folder made from the second app of a row sits where the target was", () => {
    const start = layout({ pinned: ["a", "b", "c"], order: ["a", "b", "c"] });
    const next = folders.create(start, ctx, spec({ source: "a", target: "c" }));
    assert.deepEqual(plain(next.order), ["b", "folder:a"]);
    assert.deepEqual(plain(next.folders[0].apps), ["c", "a"]);
});

test("creating a folder pins both apps, even a running app that was not pinned", () => {
    const start = layout({ pinned: ["a"], order: ["a"] });
    const next = folders.create(start, ctx, spec({ visible: ["a", "b"] }));
    assert.deepEqual(plain(next.pinned), ["a", "b"]);
});

test("order entries written as a window class or a desktop id are found", () => {
    const start = layout({
        pinned: ["firefox", "org.gnome.Nautilus"],
        order: ["Firefox", "org.gnome.Nautilus.desktop", "z"],
    });
    const next = folders.create(
        start,
        ctx,
        spec({
            source: "Firefox",
            target: "org.gnome.Nautilus.desktop",
            visible: ["Firefox", "org.gnome.Nautilus.desktop", "z"],
        }),
    );
    assert.deepEqual(plain(next.order), ["folder:a", "z"]);
    assert.deepEqual(plain(next.folders[0].apps), ["org.gnome.Nautilus", "firefox"]);
});

test("create refuses an app on itself, a folder as a member, a member of another folder and a full set", () => {
    const start = layout({
        pinned: ["a", "b", "c", "d"],
        order: ["folder:x", "c"],
        folders: [folder("folder:x", ["a", "b"])],
    });
    assert.equal(folders.create(start, ctx, spec({ source: "c", target: "c" })), null);
    assert.equal(folders.create(start, ctx, spec({ source: "folder:x", target: "c" })), null);
    assert.equal(folders.create(start, ctx, spec({ source: "c", target: "a", id: "folder:y" })), null);
    const many = Array.from({ length: 64 }, (_, index) => folder("folder:f" + index, ["p" + index, "q" + index]));
    assert.equal(folders.create(layout({ folders: many }), ctx, spec({ id: "folder:new" })), null);
});

test("a new id has the shape the schema accepts and never repeats", () => {
    const existing = [folder(folders.newId([], 1_000_000_000_000), ["a", "b"])];
    assert.match(existing[0].id, /^folder:[a-z0-9]+$/);
    const clash = folders.newId(existing, 1_000_000_000_000);
    assert.notEqual(clash, existing[0].id);
    assert.match(clash, /^folder:[a-z0-9]+$/);
});

test("adding an app puts it at the index, unpins nothing, and hides it from the order", () => {
    const start = layout({
        pinned: ["a", "b"],
        order: ["folder:x", "c", "d"],
        folders: [folder("folder:x", ["a", "b"])],
    });
    const next = folders.addTo(start, ctx, "folder:x", "d", 1);
    assert.deepEqual(plain(next.folders[0].apps), ["a", "d", "b"]);
    assert.deepEqual(plain(next.order), ["folder:x", "c"]);
    assert.ok(next.pinned.includes("d"), "a member is pinned");
});

test("an index past the end appends and a negative one prepends", () => {
    const start = layout({ order: ["folder:x", "c", "d"], folders: [folder("folder:x", ["a", "b"])] });
    assert.deepEqual(plain(folders.addTo(start, ctx, "folder:x", "c", 99).folders[0].apps), ["a", "b", "c"]);
    assert.deepEqual(plain(folders.addTo(start, ctx, "folder:x", "c", -4).folders[0].apps), ["c", "a", "b"]);
    assert.deepEqual(plain(folders.addTo(start, ctx, "folder:x", "c").folders[0].apps), ["a", "b", "c"]);
});

test("an app already in the folder, an unknown folder and a folder as an app are refused", () => {
    const start = layout({ folders: [folder("folder:x", ["a", "b"])] });
    assert.equal(folders.addTo(start, ctx, "folder:x", "a", 0), null);
    assert.equal(folders.addTo(start, ctx, "folder:missing", "c", 0), null);
    assert.equal(folders.addTo(start, ctx, "folder:x", "folder:x", 0), null);
});

test("a folder cannot grow past the schema's 256 members", () => {
    const apps = Array.from({ length: 256 }, (_, index) => "app" + index);
    const start = layout({ folders: [folder("folder:x", apps)] });
    assert.equal(folders.addTo(start, ctx, "folder:x", "extra", 0), null);
});

test("moving an app between folders leaves the first folder collapsed when it falls below two", () => {
    const start = layout({
        pinned: ["a", "b", "c", "d"],
        order: ["folder:x", "folder:y"],
        folders: [folder("folder:x", ["a", "b"]), folder("folder:y", ["c", "d"])],
    });
    const next = folders.addTo(start, ctx, "folder:y", "a", 0);
    assert.deepEqual(plain(next.folders.map((item) => item.id)), ["folder:y"]);
    assert.deepEqual(plain(next.folders[0].apps), ["a", "c", "d"]);
    assert.deepEqual(plain(next.order), ["b", "folder:y"], "b takes the slot of the folder it was left alone in");
});

test("removing an app puts it just after its folder", () => {
    const start = layout({
        pinned: ["a", "b", "c"],
        order: ["x", "folder:x", "y"],
        folders: [folder("folder:x", ["a", "b", "c"])],
    });
    const next = folders.removeFrom(start, ctx, "folder:x", "b");
    assert.deepEqual(plain(next.folders[0].apps), ["a", "c"]);
    assert.deepEqual(plain(next.order), ["x", "folder:x", "b", "y"]);
    assert.ok(next.pinned.includes("b"), "an app taken out stays pinned");
});

test("removing an app to a chosen slot puts it there", () => {
    const start = layout({ order: ["x", "folder:x", "y"], folders: [folder("folder:x", ["a", "b", "c"])] });
    const next = folders.removeFrom(start, ctx, "folder:x", "a", 0);
    assert.deepEqual(plain(next.order), ["a", "x", "folder:x", "y"]);
});

test("a folder left with one app is replaced by it and the removed app follows", () => {
    const start = layout({
        pinned: ["a", "b"],
        order: ["x", "folder:x", "y"],
        folders: [folder("folder:x", ["a", "b"])],
    });
    const next = folders.removeFrom(start, ctx, "folder:x", "a");
    assert.deepEqual(plain(next.folders), []);
    assert.deepEqual(plain(next.order), ["x", "b", "a", "y"]);
});

test("removing the last app of a one-app folder from a file written by an older version removes the folder", () => {
    const start = layout({ order: ["folder:x"], folders: [folder("folder:x", ["a"])] });
    const next = folders.removeFrom(start, ctx, "folder:x", "a");
    assert.deepEqual(plain(next.folders), []);
    assert.deepEqual(plain(next.order), ["a"]);
});

test("removing an app that is not a member is refused", () => {
    const start = layout({ folders: [folder("folder:x", ["a", "b"])] });
    assert.equal(folders.removeFrom(start, ctx, "folder:x", "z"), null);
    assert.equal(folders.removeFrom(start, ctx, "folder:none", "a"), null);
});

test("reordering moves a member and keeps the others in order", () => {
    const start = layout({ folders: [folder("folder:x", ["a", "b", "c", "d"])] });
    assert.deepEqual(plain(folders.reorder(start, ctx, "folder:x", "a", 2).folders[0].apps), ["b", "c", "a", "d"]);
    assert.deepEqual(plain(folders.reorder(start, ctx, "folder:x", "d", 0).folders[0].apps), ["d", "a", "b", "c"]);
    assert.deepEqual(plain(folders.reorder(start, ctx, "folder:x", "b", 99).folders[0].apps), ["a", "c", "d", "b"]);
});

test("reordering to the same place or an unknown app changes nothing", () => {
    const start = layout({ folders: [folder("folder:x", ["a", "b"])] });
    assert.equal(folders.reorder(start, ctx, "folder:x", "a", 0), null);
    assert.equal(folders.reorder(start, ctx, "folder:x", "z", 0), null);
});

test("renaming trims, keeps within the schema's limits and refuses the rest", () => {
    const start = layout({ folders: [folder("folder:x", ["a", "b"], { name: "Old" })] });
    assert.equal(folders.rename(start, "folder:x", "  Work  ").folders[0].name, "Work");
    assert.equal(folders.rename(start, "folder:x", "   "), null);
    assert.equal(folders.rename(start, "folder:x", "Old"), null);
    assert.equal(folders.rename(start, "folder:x", "bad\nname"), null);
    assert.equal(folders.rename(start, "folder:x", "x".repeat(65)), null);
    assert.equal(folders.rename(start, "folder:x", "x".repeat(64)).folders[0].name.length, 64);
    assert.equal(folders.rename(start, "folder:none", "Work"), null);
});

test("a colour must be one the schema lists", () => {
    const start = layout({ folders: [folder("folder:x", ["a", "b"], { color: "blue" })] });
    assert.equal(folders.recolour(start, "folder:x", "green").folders[0].color, "green");
    assert.equal(folders.recolour(start, "folder:x", "magenta"), null);
    assert.equal(folders.recolour(start, "folder:x", "blue"), null);
    for (const colour of folders.COLOURS)
        assert.ok(folders.recolour(start, "folder:x", colour === "blue" ? "red" : colour));
});

test("ungrouping puts the apps in the folder's slot in folder order", () => {
    const start = layout({
        pinned: ["a", "b", "c"],
        order: ["x", "folder:x", "y"],
        folders: [folder("folder:x", ["c", "a", "b"])],
    });
    const next = folders.ungroup(start, ctx, "folder:x");
    assert.deepEqual(plain(next.folders), []);
    assert.deepEqual(plain(next.order), ["x", "c", "a", "b", "y"]);
    assert.deepEqual(plain(next.pinned), ["a", "b", "c"], "ungrouped apps stay pinned");
});

test("normalise dissolves empty and one-app folders from a file that holds them", () => {
    const start = layout({
        pinned: ["a", "b", "c"],
        order: ["folder:empty", "folder:one", "folder:two"],
        folders: [folder("folder:empty", []), folder("folder:one", ["a"]), folder("folder:two", ["b", "c"])],
    });
    const next = folders.normalise(start, ctx);
    assert.deepEqual(plain(next.folders.map((item) => item.id)), ["folder:two"]);
    assert.deepEqual(plain(next.order), ["a", "folder:two"]);
});

test("normalise removes a duplicate membership, a stale folder id and a hidden member from the order", () => {
    const start = layout({
        pinned: ["a", "b", "c"],
        order: ["a", "folder:gone", "folder:x", "folder:x"],
        folders: [folder("folder:x", ["a", "b"]), folder("folder:y", ["b", "c"])],
    });
    const next = folders.normalise(start, ctx);
    assert.deepEqual(plain(next.folders.map((item) => item.id)), ["folder:x"], "y keeps only c and collapses");
    assert.deepEqual(plain(next.order), ["folder:x", "c"]);
});

test("normalise is idempotent and does not change a healthy layout", () => {
    const healthy = layout({
        pinned: ["a", "b", "c"],
        order: ["folder:x", "c"],
        folders: [folder("folder:x", ["a", "b"])],
    });
    assert.deepEqual(plain(folders.normalise(healthy, ctx)), plain(healthy));
    const messy = layout({ order: ["folder:one"], folders: [folder("folder:one", ["a"])] });
    const once = folders.normalise(messy, ctx);
    assert.deepEqual(plain(folders.normalise(once, ctx)), plain(once));
});

test("the saved shape survives a round trip through JSON and the schema's constraints", () => {
    const start = layout({ pinned: ["a", "b", "c"], order: ["a", "b", "c"] });
    const made = folders.create(start, ctx, spec({ id: folders.newId([], 1_700_000_000_000), name: "Work" }));
    const saved = JSON.parse(JSON.stringify(made));
    assert.deepEqual(Object.keys(saved.folders[0]).sort(), ["apps", "color", "id", "name"]);
    assert.match(saved.folders[0].id, /^folder:[a-z0-9]+$/);
    assert.ok(folders.COLOURS.includes(saved.folders[0].color));
    assert.equal(new Set(saved.order).size, saved.order.length, "order has no duplicates");
    assert.equal(new Set(saved.pinned).size, saved.pinned.length, "pinned has no duplicates");
    assert.deepEqual(plain(folders.normalise(saved, ctx)), saved);
});

test("the closed folder shows 2 by 2 up to four apps and 3 by 3 beyond", () => {
    assert.deepEqual(plain(folders.preview(2, 56)), { columns: 2, shown: 2, more: 0 });
    assert.deepEqual(plain(folders.preview(4, 56)), { columns: 2, shown: 4, more: 0 });
    assert.deepEqual(plain(folders.preview(5, 56)), { columns: 3, shown: 5, more: 0 });
    assert.deepEqual(plain(folders.preview(9, 56)), { columns: 3, shown: 9, more: 0 });
});

test("a folder that does not fit its grid reserves the last cell for how many more", () => {
    assert.deepEqual(plain(folders.preview(10, 56)), { columns: 3, shown: 8, more: 2 });
    assert.deepEqual(plain(folders.preview(40, 80)), { columns: 3, shown: 8, more: 32 });
    // A small dock keeps 2 by 2: three apps and a "+N" cell.
    assert.deepEqual(plain(folders.preview(7, 32)), { columns: 2, shown: 3, more: 4 });
    assert.deepEqual(plain(folders.preview(5, 40)), { columns: 2, shown: 3, more: 2 });
});

test("pages hold nine apps", () => {
    assert.equal(folders.pageCount(0), 1);
    assert.equal(folders.pageCount(9), 1);
    assert.equal(folders.pageCount(10), 2);
    assert.equal(folders.pageCount(19), 3);
    assert.equal(folders.pageOf(8), 0);
    assert.equal(folders.pageOf(9), 1);
});

test("arrow keys move over the grid and stop at its edges", () => {
    assert.equal(folders.step(0, "right", 5), 1);
    assert.equal(folders.step(2, "right", 5), 3, "right wraps to the next row");
    assert.equal(folders.step(4, "right", 5), 4);
    assert.equal(folders.step(0, "left", 5), 0);
    assert.equal(folders.step(3, "left", 5), 2);
    assert.equal(folders.step(4, "up", 5), 1);
    assert.equal(folders.step(1, "up", 5), 1);
    assert.equal(folders.step(0, "down", 5), 3);
    assert.equal(folders.step(2, "down", 5), 4, "a short last row ends on its last app");
    assert.equal(folders.step(4, "down", 5), 4);
    assert.equal(folders.step(1, "home", 5), 0);
    assert.equal(folders.step(1, "end", 5), 4);
    assert.equal(folders.step(0, "down", 0), -1);
    assert.equal(folders.step(-1, "down", 5), 3, "no selection starts from the first app");
});
