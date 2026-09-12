import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
const history = {},
    match = {};
vm.runInNewContext(readFileSync(new URL("../shell/bingux/NotificationHistory.js", import.meta.url), "utf8"), history);
vm.runInNewContext(readFileSync(new URL("../shell/bingux/MediaMatch.js", import.meta.url), "utf8"), match);
const entry = {
    notification: { id: 1 },
    historyKey: "old:1",
    receivedAt: 12,
    desktopEntry: "discord-canary",
    appName: "Discord Canary",
    summary: "Marc",
    body: "Cooking",
    image: "file:///avatar.png",
    actions: [{ action: { invoke() {} } }],
};
test("restart history retains dock identity and images without stale actions or replayed toasts", () => {
    const restored = history.restore(history.encode([entry], "old"), [], "new");
    assert.equal(restored.length, 1);
    assert.equal(restored[0].image, entry.image);
    assert.equal(restored[0].toastVisible, false);
    assert.equal(restored[0].actions.length, 0);
    assert.ok(restored[0].notification.id < 0);
    assert.ok(match.matchesNotification(restored[0], { desktopEntry: { id: "discord-canary" } }));
    const live = { ...entry, historyKey: "new:1", summary: "New" };
    assert.equal(history.restore(history.encode(restored, "new"), [live], "new").length, 2);
    assert.equal(history.restore(history.encode(restored, "new"), restored, "new").length, 1);
});
test("ordinary images stay small while explicit screenshots retain previews", () => {
    assert.equal(history.largeImage(entry), false);
    assert.equal(history.largeImage({ ...entry, appName: "Capture", summary: "Screenshot saved" }), true);
});
test("bad history cannot replace current notifications", () => {
    assert.equal(history.restore("{bad", [entry], "new")[0], entry);
    assert.equal(history.restore('{"version":9,"entries":[]}', [entry], "new")[0], entry);
});

test("large history preserves entries and deduplicates within the saved batch", () => {
    const entries = Array.from({ length: 10000 }, (_, i) => ({ key: "old:" + i, summary: "Saved", receivedAt: i }));
    const result = history.restore(JSON.stringify({ version: 1, entries: entries.concat(entries) }), [], "new");
    assert.equal(result.length, 10000);
    assert.equal(new Set(result.map((entry) => entry.notification.id)).size, 10000);
    assert.equal(result[0].receivedAt, 9999);
});
