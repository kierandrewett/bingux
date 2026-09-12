import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const context = vm.createContext({});
vm.runInContext(
    readFileSync(new URL("../shell/bingux/SearchGroups.js", import.meta.url), "utf8").replace(".pragma library", ""),
    context,
);

test("groups interleaved sources while retaining within-group ranking", () => {
    const rows = [
        { resultId: "web", providerId: "web", kind: "action" },
        { resultId: "file", providerId: "files", kind: "file" },
        { resultId: "app-best", providerId: "applications", kind: "application" },
        { resultId: "folder", providerId: "files", kind: "folder" },
        { resultId: "app-next", providerId: "applications", kind: "application" },
    ];
    assert.deepEqual(
        Array.from(context.ordered(rows), (row) => row.resultId),
        ["app-best", "app-next", "file", "folder", "web"],
    );
    assert.equal(rows[0].resultId, "web");
});
test("answers come first and custom providers get their own group", () => {
    const rows = [
        { providerId: "web", kind: "action" },
        { providerId: "my-notes", kind: "database" },
        { providerId: "applications", kind: "application" },
        { providerId: "calculation", kind: "calculation" },
    ];
    assert.deepEqual(
        Array.from(context.ordered(rows), (row) => context.key(row)),
        ["calculation", "applications", "my-notes", "web"],
    );
    assert.equal(context.label("my-notes"), "My Notes");
});

test("suggestions share the Web group with the original search action", () => {
    assert.equal(context.key({ providerId: "web-suggestions", kind: "action" }), "web");
});

test("grouping preserves relevance even with an unstable runtime sort", () => {
    const unstable = vm.createContext({});
    vm.runInContext(
        `
        const originalSort = Array.prototype.sort;
        Array.prototype.sort = function(compare) {
            const original = this.slice();
            return originalSort.call(this, (a, b) => compare(a, b) || original.indexOf(b) - original.indexOf(a));
        };
    `,
        unstable,
    );
    vm.runInContext(
        readFileSync(new URL("../shell/bingux/SearchGroups.js", import.meta.url), "utf8").replace(
            ".pragma library",
            "",
        ),
        unstable,
    );
    const ids = vm.runInContext(
        `ordered([
        {resultId: 'exact-folder', providerId: 'files', kind: 'folder'},
        {resultId: 'deep-copy', providerId: 'files', kind: 'folder'},
        {resultId: 'partial-zip', providerId: 'files', kind: 'file'},
        {resultId: 'web', providerId: 'web', kind: 'action'}
    ]).map(row => row.resultId)`,
        unstable,
    );
    assert.deepEqual(Array.from(ids), ["exact-folder", "deep-copy", "partial-zip", "web"]);
});
