import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../shell/bingux/NotificationState.qml", import.meta.url), "utf8");
function fixture() {
    const context = vm.createContext({
        historyReady: true,
        historyDirectory: "/history",
        imageCaptures: new Set(),
        capturedImages: new Map(),
        allEntries: [],
        imageCaptureCommit: {
            running: false,
            start() {
                this.running = true;
            },
        },
    });
    vm.runInContext(
        "var root = this;\n" +
            source.slice(
                source.indexOf("    function commitCapturedImages"),
                source.indexOf("    property var applicationAliases"),
            ),
        context,
    );
    return context;
}
test("hidden and zero-sized previews do not attempt GPU readback", () => {
    const s = fixture();
    const entry = { historyKey: "a", image: "image://pixels/a" };
    for (const dimensions of [
        { visible: false, width: 40, height: 40 },
        { visible: true, width: 0, height: 40 },
        { visible: true, width: 40, height: 0 },
    ]) {
        s.cacheImage(entry, {
            ...dimensions,
            opacity: 1,
            grabToImage() {
                assert.fail("unexpected readback");
            },
        });
    }
});
test("duplicate views share a capture and completion updates history once", () => {
    const s = fixture();
    const entry = { historyKey: "a", image: "image://pixels/a" };
    s.allEntries = [entry];
    let grabs = 0,
        complete;
    const item = {
        visible: true,
        width: 40,
        height: 40,
        opacity: 1,
        grabToImage(callback) {
            grabs++;
            complete = callback;
            return true;
        },
    };
    s.cacheImage(entry, item);
    s.cacheImage(entry, item);
    assert.equal(grabs, 1);
    complete({ saveToFile: () => true });
    s.cacheImage(entry, item);
    assert.equal(grabs, 1);
    assert.equal(s.allEntries[0], entry);
    s.commitCapturedImages();
    assert.equal(s.allEntries[0].image, "file:///history/a.png");
    assert.equal(s.imageCaptures.size, 0);
});
test("failed readbacks retry and stale completions preserve replacement images", () => {
    const s = fixture();
    const entry = { historyKey: "a", image: "image://pixels/a" };
    const item = { visible: true, width: 40, height: 40, opacity: 1, grabToImage: () => false };
    s.cacheImage(entry, item);
    assert.equal(s.imageCaptures.size, 0);
    s.allEntries = [{ ...entry, image: "image://pixels/new" }];
    item.grabToImage = (callback) => {
        callback({ saveToFile: () => true });
        return true;
    };
    s.cacheImage(entry, item);
    s.commitCapturedImages();
    assert.equal(s.allEntries[0].image, "image://pixels/new");
});
