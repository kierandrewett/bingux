import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
const source = readFileSync(new URL("../shell/bingux/NotificationStack.qml", import.meta.url), "utf8");
const fn = source.slice(source.indexOf("    function syncEntries()"), source.indexOf("    function removeCard"));
function fixture(ids, collapseOnDismiss = false) {
    let reads = 0,
        writes = 0;
    const rows = ids.map((notificationId) => ({ notificationId, retiring: false }));
    const context = {
        presentedEntries: [],
        appKey: (e) => e.app,
        expandedApps: {},
        lastSyncedEntries: new Map(),
        collapseOnDismiss,
        layoutTimer: { restart() {} },
        cardRepeater: {
            itemAt() {
                return null;
            },
        },
        cards: {
            get count() {
                return rows.length;
            },
            get(i) {
                reads++;
                return rows[i];
            },
            set(i, r) {
                writes++;
                rows[i] = r;
            },
            setProperty(i, k, v) {
                writes++;
                rows[i][k] = v;
            },
            insert(i, r) {
                rows.splice(i, 0, r);
            },
            move(from, to) {
                rows.splice(to, 0, ...rows.splice(from, 1));
            },
        },
    };
    vm.createContext(context);
    vm.runInContext(fn, context);
    return {
        rows,
        reads: () => reads,
        writes: () => writes,
        sync(entries) {
            context.presentedEntries = entries;
            context.syncEntries();
        },
    };
}
const entry = (id, app = "same") => ({ notification: { id }, app });
test("unchanged large history uses linear model reads and correct group depths", () => {
    const ids = Array.from({ length: 10000 }, (_, i) => i),
        f = fixture(ids);
    f.sync(ids.map((id) => entry(id)));
    assert.ok(f.reads() <= ids.length * 3);
    assert.equal(f.rows[9999].groupDepth, 9999);
    assert.equal(f.rows[0].groupHead, true);
});
test("grouping, reordering and new entries retain the requested order", () => {
    const f = fixture([1, 2, 3]);
    f.sync([entry(3, "a"), entry(4, "b"), entry(1, "a"), entry(2, "b")]);
    assert.deepEqual(
        f.rows.map((r) => r.notificationId),
        [3, 1, 4, 2],
    );
    assert.deepEqual(
        f.rows.map((r) => r.groupDepth),
        [0, 1, 0, 1],
    );
});
test("retiring cards stay in place during collapse and can be restored", () => {
    const f = fixture([1, 2, 3], true);
    f.sync([entry(1), entry(3)]);
    assert.equal(f.rows[1].retiring, true);
    assert.deepEqual(
        f.rows.map((r) => r.notificationId),
        [1, 2, 3],
    );
    f.sync([entry(1), entry(2), entry(3)]);
    assert.equal(f.rows[1].retiring, false);
});

test("unchanged entries are not reassigned during history reconciliation", () => {
    const entries = Array.from({ length: 100 }, (_, id) => entry(id));
    const f = fixture(entries.map((e) => e.notification.id));
    f.sync(entries);
    const writes = f.writes();
    f.sync(entries);
    assert.equal(f.writes(), writes);
    const changed = { ...entries[50], summary: "Updated" };
    f.sync(entries.map((e) => (e.notification.id === 50 ? changed : e)));
    assert.equal(f.writes(), writes + 1);
    assert.equal(f.rows[50].entryData, changed);
});
