import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

// BsmReadiness.js decides what Bingux tells bsm's bridge. The properties that
// matter: ready is never sent before every required surface rendered a frame,
// surface lines never precede launched, optional surfaces never block, and a
// reconnect reports everything again.
const source = readFileSync(new URL("../shell/bingux/BsmReadiness.js", import.meta.url), "utf8");
const R = {};
vm.runInNewContext(source, R, { filename: "BsmReadiness.js" });
const plain = (value) => JSON.parse(JSON.stringify(value));

const GEN = "42";
const win = (role, output, rendered) => ({ role, output, rendered });
const desktop = (windows, dock = true, loaded = true) =>
    plain(R.evaluate({ component: "desktop", config: { loaded, dock }, screens: ["A", "B"], windows }));
const wallpaper = (windows, screens = ["A", "B"]) =>
    plain(R.evaluate({ component: "wallpaper", config: {}, screens, windows }));
const parse = (lines) => Array.from(lines, (line) => JSON.parse(line));

test("generation must be a decimal string", () => {
    for (const ok of ["0", "7", "18446744073709551615"]) assert.equal(R.validGeneration(ok), true, ok);
    for (const bad of ["", "-1", "1.5", "0x10", "abc", " 1", "1 ", "123456789012345678901", 5, null, undefined])
        assert.equal(R.validGeneration(bad), false, String(bad));
});

test("required surfaces follow the configuration", () => {
    assert.deepEqual(plain(R.requiredRoles("wallpaper", {})), ["wallpaper"]);
    assert.deepEqual(plain(R.requiredRoles("desktop", { dock: true })), ["bar", "dock"]);
    assert.deepEqual(plain(R.requiredRoles("desktop", { dock: false })), ["bar"]);
    assert.deepEqual(plain(R.requiredRoles("desktop", {})), ["bar"]);
    assert.deepEqual(plain(R.requiredRoles("popouts", { dock: true })), [], "popouts never report");
});

test("desktop: nothing is launched until preferences are read and both windows exist", () => {
    assert.equal(desktop([], true).launched, false);
    assert.equal(desktop([win("bar", "A", false), win("dock", "A", false)], true, false).launched, false);
    assert.equal(desktop([win("bar", "A", false)], true).launched, false, "dock enabled but not created");
    assert.equal(desktop([win("bar", "A", false), win("dock", "A", false)], true).launched, true);
    assert.equal(desktop([win("bar", "A", false)], false).launched, true, "dock disabled: bar alone launches");
});

test("desktop: ready needs a rendered frame on every required surface", () => {
    const bar = win("bar", "A", true);
    assert.equal(desktop([bar, win("dock", "A", false)]).ready, false);
    assert.equal(desktop([win("bar", "A", false), win("dock", "A", true)]).ready, false);
    assert.equal(desktop([bar, win("dock", "A", true)]).ready, true);
});

test("desktop: a disabled dock neither blocks nor is reported", () => {
    const evaluation = desktop([win("bar", "A", true), win("dock", "A", false)], false);
    assert.equal(evaluation.ready, true);
    assert.deepEqual(evaluation.surfaces, [{ role: "bar", output: "A", mapped: true }]);
});

test("desktop: a window that has no output yet is not launched", () => {
    assert.equal(desktop([win("bar", "", true), win("dock", "A", true)]).launched, false);
});

test("wallpaper: one surface per screen, ready only when all rendered", () => {
    assert.equal(wallpaper([], []).launched, false, "no screens, nothing to launch");
    assert.equal(wallpaper([win("wallpaper", "A", true)]).launched, false, "screen B has no window yet");
    const one = wallpaper([win("wallpaper", "A", true), win("wallpaper", "B", false)]);
    assert.equal(one.launched, true);
    assert.equal(one.ready, false);
    assert.equal(wallpaper([win("wallpaper", "A", true), win("wallpaper", "B", true)]).ready, true);
    assert.equal(wallpaper([win("wallpaper", "B", true)], ["B"]).ready, true);
});

test("wallpaper: a window on an unplugged screen does not count", () => {
    const evaluation = wallpaper([win("wallpaper", "A", true), win("wallpaper", "Z", true)], ["A"]);
    assert.equal(evaluation.ready, true);
    assert.deepEqual(
        evaluation.surfaces.map((s) => s.output),
        ["A"],
    );
});

test("lines: launched, then surfaces, then ready, each once", () => {
    const evaluation = desktop([win("bar", "A", true), win("dock", "A", true)]);
    const { lines, sent } = R.diff(GEN, "desktop", evaluation, {});
    const events = parse(lines);
    assert.deepEqual(
        events.map((e) => e.event),
        ["launched", "surface", "surface", "ready"],
    );
    assert.deepEqual(plain(events[0]), {
        generation: GEN,
        component: "desktop",
        event: "launched",
        required: ["bar", "dock"],
    });
    assert.deepEqual(plain(events[1]), {
        generation: GEN,
        component: "desktop",
        event: "surface",
        output: "A",
        role: "bar",
        mapped: true,
    });
    assert.deepEqual(plain(events[3]), { generation: GEN, component: "desktop", event: "ready" });
    // Nothing changed: nothing is sent again.
    assert.deepEqual(plain(R.diff(GEN, "desktop", evaluation, sent).lines), []);
});

test("lines: no surface line before launched, no ready before every surface", () => {
    // Dock enabled but its window is missing: bar is rendered, still nothing to say.
    const partial = desktop([win("bar", "A", true)]);
    assert.deepEqual(plain(R.diff(GEN, "desktop", partial, {}).lines), []);
    // Launched, one surface still unrendered: surfaces yes, ready no.
    const waiting = desktop([win("bar", "A", true), win("dock", "A", false)]);
    const events = parse(R.diff(GEN, "desktop", waiting, {}).lines);
    assert.deepEqual(
        events.map((e) => e.event),
        ["launched", "surface", "surface"],
    );
    assert.deepEqual(
        events.filter((e) => e.event === "surface").map((e) => e.mapped),
        [true, false],
    );
});

test("lines: only the changed surface and ready follow", () => {
    const waiting = desktop([win("bar", "A", true), win("dock", "A", false)]);
    const first = R.diff(GEN, "desktop", waiting, {});
    const done = desktop([win("bar", "A", true), win("dock", "A", true)]);
    const second = parse(R.diff(GEN, "desktop", done, first.sent).lines);
    assert.deepEqual(
        second.map((e) => e.event + ":" + (e.role || "")),
        ["surface:dock", "ready:"],
    );
    assert.equal(second[0].mapped, true);
});

test("lines: an unmapped surface is withdrawn and ready is sent again when it returns", () => {
    const ready = desktop([win("bar", "A", true), win("dock", "A", true)]);
    const first = R.diff(GEN, "desktop", ready, {});
    const gone = desktop([win("bar", "A", true), win("dock", "A", false)]);
    const second = R.diff(GEN, "desktop", gone, first.sent);
    const events = parse(second.lines);
    assert.deepEqual(
        events.map((e) => e.event),
        ["surface"],
    );
    assert.equal(events[0].role, "dock");
    assert.equal(events[0].mapped, false);
    assert.equal(second.sent.ready, undefined, "ready is forgotten while a surface is down");
    const third = parse(R.diff(GEN, "desktop", ready, second.sent).lines);
    assert.deepEqual(
        third.map((e) => e.event),
        ["surface", "ready"],
    );
});

test("lines: a removed surface (monitor unplugged) is withdrawn", () => {
    const both = wallpaper([win("wallpaper", "A", true), win("wallpaper", "B", true)]);
    const first = R.diff(GEN, "wallpaper", both, {});
    const onlyA = wallpaper([win("wallpaper", "A", true)], ["A"]);
    const events = parse(R.diff(GEN, "wallpaper", onlyA, first.sent).lines);
    assert.deepEqual(
        events.map((e) => [e.event, e.output, e.mapped]),
        [["surface", "B", false]],
    );
});

test("lines: a new monitor delays ready until its wallpaper has rendered", () => {
    const one = wallpaper([win("wallpaper", "A", true)], ["A"]);
    const first = R.diff(GEN, "wallpaper", one, {});
    assert.ok(parse(first.lines).some((e) => e.event === "ready"));
    const plugged = wallpaper([win("wallpaper", "A", true)], ["A", "B"]);
    const second = R.diff(GEN, "wallpaper", plugged, first.sent);
    assert.deepEqual(plain(second.lines), [], "B has no window yet: A stays mapped, no new claim");
    assert.equal(second.sent.ready, undefined, "ready is withheld until B has rendered");
    assert.ok(second.sent.launched, "launched stays described");
    const rendered = wallpaper([win("wallpaper", "A", true), win("wallpaper", "B", true)], ["A", "B"]);
    const third = parse(R.diff(GEN, "wallpaper", rendered, second.sent).lines);
    assert.deepEqual(
        third.map((e) => e.event + ":" + (e.output || "")),
        ["surface:B", "ready:"],
    );
});

test("lines: dock preference changing after launch re-sends launched with the new required list", () => {
    const withDock = desktop([win("bar", "A", true), win("dock", "A", true)], true);
    const first = R.diff(GEN, "desktop", withDock, {});
    const noDock = desktop([win("bar", "A", true), win("dock", "A", false)], false);
    const events = parse(R.diff(GEN, "desktop", noDock, first.sent).lines);
    const launched = events.find((e) => e.event === "launched");
    assert.deepEqual(plain(launched.required), ["bar"]);
    assert.ok(events.some((e) => e.event === "surface" && e.role === "dock" && e.mapped === false));
    assert.equal(
        events.some((e) => e.event === "ready"),
        false,
        "ready was already sent for the bar and is not repeated",
    );
});

test("a new connection starts from nothing and reports the full state", () => {
    const ready = desktop([win("bar", "A", true), win("dock", "A", true)]);
    const first = R.diff(GEN, "desktop", ready, {});
    const again = R.diff(GEN, "desktop", ready, {});
    assert.deepEqual(plain(again.lines), plain(first.lines));
});

test("every line is one compact JSON object under 4 KiB with the common fields", () => {
    const ready = desktop([win("bar", "Virtual-1", true), win("dock", "Virtual-1", true)]);
    for (const line of R.diff(GEN, "desktop", ready, {}).lines) {
        assert.ok(!line.includes("\n"));
        assert.ok(line.length < 4096);
        const object = JSON.parse(line);
        assert.equal(object.generation, GEN);
        assert.equal(object.component, "desktop");
        assert.ok(["launched", "surface", "ready"].includes(object.event));
    }
});

test("the readiness code never reads Startup.settled", () => {
    for (const file of ["BsmReadiness.js", "BsmReadiness.qml", "BsmSurfaceProbe.qml", "BsmDesktop.qml"]) {
        const text = readFileSync(new URL("../shell/bingux/" + file, import.meta.url), "utf8");
        const code = text.split("\n").filter((line) => !line.trim().startsWith("//")).join("\n");
        assert.ok(!/Startup\./.test(code), file + " uses Startup");
    }
});
