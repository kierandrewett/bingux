import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const context = vm.createContext({});
vm.runInContext(fs.readFileSync(new URL("../shell/bingux/CaptureOptions.js", import.meta.url), "utf8"), context);
const bounds = {width: 1920, height: 1080};
test("accepts capture options and an exact screen-edge region", () => {
    const options = {fps: 15, copy: false, directory: "/tmp/captures", region: {x: 0, y: 0, width: 1920, height: 1080}};
    assert.equal(context.validate(options, bounds), options);
});
test("rejects malformed and unknown options before the caller can apply them", () => {
    for (const options of [{fps: 24}, {copy: "false"}, {directory: "relative"}, {audio: "bad"},
        {region: {x: 1900, y: 0, width: 30, height: 2}}, {region: {x: 0, y: 0, width: 2, height: 2, extra: 1}},
        {constructor: "bad"}, JSON.parse('{"__proto__":{}}'), [], null]) {
        assert.throws(() => context.validate(options, bounds));
    }
});
