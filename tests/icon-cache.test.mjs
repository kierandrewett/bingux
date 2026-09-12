import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
const api = vm.createContext({});
vm.runInContext(readFileSync(new URL("../shell/bingux/IconCache.js", import.meta.url), "utf8"), api);

test("unused icons stay within both byte and entry budgets", () => {
    const cache = api.create(100, 2);
    for (const id of ["a", "b", "c"]) api.touch(cache, id, 40);
    assert.deepEqual(Array.from(api.trim(cache)), ["a"]);
    api.touch(cache, "b");
    api.touch(cache, "d", 40);
    assert.deepEqual(Array.from(api.trim(cache)), ["c"]);
    assert.equal(api.stats(cache).unusedStringBytes, 80);
});

test("shared active icons survive eviction until their last user releases them", () => {
    const cache = api.create(100, 2);
    api.retain(cache, "active");
    api.retain(cache, "active");
    api.touch(cache, "active", 1000);
    api.release(cache, "active");
    assert.equal(api.trim(cache).length, 0);
    api.release(cache, "active");
    assert.deepEqual(Array.from(api.trim(cache)), ["active"]);
    assert.equal(cache.sizes.size, 0);
});

test("unused requests without responses are also bounded", () => {
    const cache = api.create(100, 2);
    for (let i = 0; i < 5000; i++) {
        api.touch(cache, String(i));
        api.trim(cache);
    }
    assert.equal(cache.unused.size, 2);
});
