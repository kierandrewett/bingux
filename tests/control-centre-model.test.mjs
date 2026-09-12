import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
import assert from "node:assert/strict";
const model = vm.createContext({});
vm.runInContext(readFileSync(new URL("../shell/bingux/ControlCentreModel.js", import.meta.url), "utf8"), model);
test("normal network view prioritises the connection in use and avoids duplicate Wi-Fi entries", () => {
    const connections = [
        { name: "Home", type: "802-11-wireless", connected: true },
        { name: "tailscale0", type: "tun", connected: true },
        { name: "Old network", type: "802-11-wireless", connected: false },
    ];
    const sections = model.networkSections(connections, [
        { name: "Home", connected: true },
        { name: "Cafe", connected: false },
    ]);
    assert.deepEqual(
        Array.from(sections.current, (item) => item.name),
        ["Home"],
    );
    assert.deepEqual(
        Array.from(sections.nearby, (item) => item.name),
        ["Cafe"],
    );
    assert.deepEqual(
        Array.from(sections.other, (item) => item.name),
        ["Old network"],
    );
    assert.equal(model.connectionName(connections[1]), "Tailscale");
});
test("offline profiles remain available without claiming a current connection", () => {
    const sections = model.networkSections([{ name: "Home", type: "802-11-wireless", connected: false }], []);
    assert.equal(sections.current.length, 0);
    assert.equal(sections.other.length, 1);
});

test("Bluetooth devices use symbolic glyphs rather than full-colour icon backgrounds", () => {
    assert.equal(model.bluetoothIcon("audio-headphones"), "audio-headphones-symbolic");
    assert.equal(model.bluetoothIcon("input-keyboard-symbolic"), "input-keyboard-symbolic");
    assert.equal(model.bluetoothIcon("vendor-device-picture"), "bluetooth-active-symbolic");
    assert.equal(model.bluetoothIcon(""), "bluetooth-active-symbolic");
});
