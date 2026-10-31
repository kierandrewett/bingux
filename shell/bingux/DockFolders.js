.pragma library

// The dock's folder model: pure functions over a layout, no QML and no I/O.
//
// A layout is { pinned, order, folders }. `pinned` and `order` are the saved
// `desktop.dock_apps` lists; `folders` is `desktop.dock_folders` (id, name,
// colour and apps of each). A folder member is an application identity, the
// desktop entry id without ".desktop". `order` holds the ids of the slots the
// dock shows: an application's id or identity, or a folder's id.
//
// Every function returns a new layout and never changes its argument. The
// functions that can refuse return null, and the caller then leaves the saved
// layout alone. The caller passes `identityOf(id)`, which maps any id the dock
// uses for an application (group id, desktop id, identity) to its identity, so
// `order` entries written in either form are found.
//
// The collapse rule, applied by every function that removes a member: a folder
// with fewer than two apps is dissolved. An empty folder disappears. A folder
// that keeps one app is replaced, in its dock slot, by that app. A dock slot
// costs width, and a one-app folder only adds a click to reach the app.

var COLOURS = ["blue", "teal", "green", "yellow", "orange", "red", "pink", "purple", "slate"];
var DEFAULT_NAME = "Folder";
var MINIMUM_MEMBERS = 2;
// Limits of the configuration schema (crates/config validate.rs).
var MAXIMUM_FOLDERS = 64;
var MAXIMUM_MEMBERS = 256;
var MAXIMUM_NAME = 64;
var PAGE_SIZE = 9;
var PAGE_COLUMNS = 3;

function isFolderId(id) {
    return typeof id === "string" && id.indexOf("folder:") === 0;
}

// A new id: "folder:" and lower-case base 36, the shape the schema accepts.
// `stamp` is a number the caller supplies (the time); a clash is lengthened.
function newId(folders, stamp) {
    var id = "folder:" + Math.floor(stamp).toString(36);
    while (find(folders, id)) id += "0";
    return id;
}

function find(folders, id) {
    for (var index = 0; index < folders.length; index++) if (folders[index].id === id) return folders[index];
    return null;
}

// The folder that holds the application, or null.
function folderOf(folders, identity) {
    for (var index = 0; index < folders.length; index++)
        if (folders[index].apps.indexOf(identity) >= 0) return folders[index];
    return null;
}

function clone(layout) {
    return {
        pinned: layout.pinned.slice(),
        order: layout.order.slice(),
        folders: layout.folders.map(function (folder) {
            return {
                id: folder.id,
                name: folder.name,
                color: folder.color,
                apps: folder.apps.slice(),
            };
        }),
    };
}

function union(list, additions) {
    var result = list.slice();
    additions.forEach(function (item) {
        if (result.indexOf(item) < 0) result.push(item);
    });
    return result;
}

// Removes every order entry for an application, whichever form it is in.
function withoutApp(order, identity, ctx) {
    return order.filter(function (entry) {
        return isFolderId(entry) || ctx.identityOf(entry) !== identity;
    });
}

// The folder's slot: where the folder id sits in the order, or the end.
function slotOf(order, folderId) {
    var index = order.indexOf(folderId);
    return index < 0 ? order.length : index;
}

function replaceSlot(order, folderId, replacements) {
    var index = slotOf(order, folderId);
    var next = order.filter(function (entry) {
        return entry !== folderId;
    });
    next.splice(Math.min(index, next.length), 0, ...replacements);
    return next;
}

// Dissolves folders with fewer than two apps and removes the order entries and
// duplicate memberships a hand-edited or older file can carry. Idempotent.
function normalise(layout, ctx) {
    var next = clone(layout);
    var seen = [];
    var kept = [];
    next.folders.forEach(function (folder) {
        folder.apps = folder.apps.filter(function (app) {
            if (seen.indexOf(app) >= 0) return false;
            seen.push(app);
            return true;
        });
        if (folder.apps.length >= MINIMUM_MEMBERS) kept.push(folder);
        else next.order = replaceSlot(next.order, folder.id, folder.apps);
    });
    next.folders = kept;
    next.order = next.order.filter(function (entry, index) {
        if (isFolderId(entry)) return !!find(next.folders, entry) && next.order.indexOf(entry) === index;
        // A member is shown by its folder.
        return !folderOf(next.folders, ctx.identityOf(entry));
    });
    next.pinned = union(
        next.pinned,
        seen.filter(function (app) {
            return !!folderOf(next.folders, app);
        }),
    );
    return next;
}

// Drops `source` onto `target` and makes a folder of the two at the target's
// slot. `visible` lists the ids of the dock's slots in display order.
// spec: { id, name, color, source, target, visible }.
function create(layout, ctx, spec) {
    var source = ctx.identityOf(spec.source);
    var target = ctx.identityOf(spec.target);
    if (!source || !target || source === target || isFolderId(spec.source) || isFolderId(spec.target)) return null;
    if (layout.folders.length >= MAXIMUM_FOLDERS || find(layout.folders, spec.id)) return null;
    if (folderOf(layout.folders, source) || folderOf(layout.folders, target)) return null;
    var next = clone(layout);
    var order = [];
    var placed = false;
    spec.visible.forEach(function (entry) {
        var identity = isFolderId(entry) ? entry : ctx.identityOf(entry);
        if (identity === source) return;
        if (identity === target) {
            order.push(spec.id);
            placed = true;
        } else {
            order.push(entry);
        }
    });
    if (!placed) order.push(spec.id);
    next.order.forEach(function (entry) {
        var identity = isFolderId(entry) ? entry : ctx.identityOf(entry);
        if (
            identity !== source &&
            identity !== target &&
            !order.some(function (kept) {
                return (isFolderId(kept) ? kept : ctx.identityOf(kept)) === identity;
            })
        )
            order.push(entry);
    });
    next.order = order;
    next.pinned = union(next.pinned, [target, source]);
    next.folders.push({
        id: spec.id,
        name: validName(spec.name) || DEFAULT_NAME,
        color: COLOURS.indexOf(spec.color) >= 0 ? spec.color : COLOURS[0],
        apps: [target, source],
    });
    return normalise(next, ctx);
}

// Adds an application to a folder at `index`. An app that is in another folder
// moves, and that folder collapses if it falls below two.
function addTo(layout, ctx, folderId, app, index) {
    var identity = ctx.identityOf(app);
    var folder = find(layout.folders, folderId);
    if (!folder || !identity || isFolderId(app)) return null;
    var next = clone(layout);
    folder = find(next.folders, folderId);
    var previous = folderOf(next.folders, identity);
    if (previous === folder) return null;
    if (folder.apps.length >= MAXIMUM_MEMBERS) return null;
    if (previous)
        previous.apps = previous.apps.filter(function (member) {
            return member !== identity;
        });
    var at =
        index === undefined || index === null ? folder.apps.length : Math.max(0, Math.min(folder.apps.length, index));
    folder.apps.splice(at, 0, identity);
    next.order = withoutApp(next.order, identity, ctx);
    next.pinned = union(next.pinned, [identity]);
    return normalise(next, ctx);
}

// Takes an application out of a folder. It lands in the dock just after the
// folder, or at `slot` (an index into `order`) when the caller drops it
// somewhere. A folder left with one app is replaced by that app, and the
// removed app follows it.
function removeFrom(layout, ctx, folderId, app, slot) {
    var identity = ctx.identityOf(app);
    var folder = find(layout.folders, folderId);
    if (!folder || folder.apps.indexOf(identity) < 0) return null;
    var next = clone(layout);
    folder = find(next.folders, folderId);
    folder.apps = folder.apps.filter(function (member) {
        return member !== identity;
    });
    var at =
        slot === undefined || slot === null
            ? slotOf(next.order, folderId) + 1
            : Math.max(0, Math.min(next.order.length, slot));
    next.order.splice(at, 0, identity);
    next.pinned = union(next.pinned, [identity]);
    return normalise(next, ctx);
}

// Moves a member to another place in its folder.
function reorder(layout, ctx, folderId, app, index) {
    var identity = ctx.identityOf(app);
    var folder = find(layout.folders, folderId);
    if (!folder) return null;
    var from = folder.apps.indexOf(identity);
    var to = Math.max(0, Math.min(folder.apps.length - 1, index));
    if (from < 0 || from === to) return null;
    var next = clone(layout);
    var apps = find(next.folders, folderId).apps;
    apps.splice(from, 1);
    apps.splice(to, 0, identity);
    return next;
}

// A name the schema accepts (1 to 64 characters, no control characters), or "".
function validName(name) {
    var text = typeof name === "string" ? name.trim() : "";
    if (text.length === 0 || Array.from(text).length > MAXIMUM_NAME || /[\u0000-\u001f\u007f]/.test(text)) return "";
    return text;
}

function rename(layout, folderId, name) {
    var text = validName(name);
    var folder = find(layout.folders, folderId);
    if (!text || !folder || folder.name === text) return null;
    var next = clone(layout);
    find(next.folders, folderId).name = text;
    return next;
}

function recolour(layout, folderId, color) {
    var folder = find(layout.folders, folderId);
    if (!folder || COLOURS.indexOf(color) < 0 || folder.color === color) return null;
    var next = clone(layout);
    find(next.folders, folderId).color = color;
    return next;
}

// Dissolves a folder. Its apps take its place in the dock, in folder order.
function ungroup(layout, ctx, folderId) {
    var folder = find(layout.folders, folderId);
    if (!folder) return null;
    var next = clone(layout);
    next.order = replaceSlot(next.order, folderId, folder.apps);
    next.folders = next.folders.filter(function (item) {
        return item.id !== folderId;
    });
    return normalise(next, ctx);
}

// Moves a folder, or any slot, to `index` in the order.
// The dock's own drag already writes the order for slots it shows; this keeps
// the folder model able to place a slot by id.
function placeSlot(layout, ctx, id, index) {
    var next = clone(layout);
    var identity = isFolderId(id) ? id : ctx.identityOf(id);
    next.order = next.order.filter(function (entry) {
        return (isFolderId(entry) ? entry : ctx.identityOf(entry)) !== identity;
    });
    next.order.splice(Math.max(0, Math.min(next.order.length, index)), 0, id);
    return next;
}

// Where a folder's mini icons go, for the closed folder.
// Up to four apps (or a small dock) use 2 by 2; more use 3 by 3. When the grid
// cannot hold every app its last cell is "+N", the apps that are not shown.
function preview(count, iconSize) {
    var columns = count <= 4 || iconSize < 44 ? 2 : 3;
    var capacity = columns * columns;
    var overflow = count > capacity;
    var shown = overflow ? capacity - 1 : Math.min(count, capacity);
    return {
        columns: columns,
        shown: shown,
        more: overflow ? count - shown : 0,
    };
}

function pageCount(count) {
    return Math.max(1, Math.ceil(count / PAGE_SIZE));
}

function pageOf(index) {
    return Math.floor(index / PAGE_SIZE);
}

// Arrow-key movement over the popup's grid of `count` apps in
// PAGE_COLUMNS columns. Returns the new index, or `index` at an edge.
// Left and Right wrap to the neighbouring row; Up and Down stay in their column
// and stop at the first and last row; Home and End jump to the first and last.
function step(index, key, count) {
    if (count <= 0) return -1;
    var current = Math.max(0, Math.min(count - 1, index));
    if (key === "left") return Math.max(0, current - 1);
    if (key === "right") return Math.min(count - 1, current + 1);
    if (key === "up") return current - PAGE_COLUMNS >= 0 ? current - PAGE_COLUMNS : current;
    if (key === "down") {
        if (current + PAGE_COLUMNS < count) return current + PAGE_COLUMNS;
        // A short last row: go to its last app when the column has no app.
        var lastRow = Math.floor((count - 1) / PAGE_COLUMNS);
        return Math.floor(current / PAGE_COLUMNS) < lastRow ? count - 1 : current;
    }
    if (key === "home") return 0;
    if (key === "end") return count - 1;
    return current;
}
