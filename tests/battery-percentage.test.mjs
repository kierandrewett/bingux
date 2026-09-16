import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
import assert from "node:assert/strict";

const source = readFileSync(new URL("../shell/bingux/SystemIndicators.qml", import.meta.url), "utf8");
const method = source.slice(source.indexOf("    function batteryAccessibleName()"), source.indexOf("    function changeVolume("));

test("battery summary converts Quickshell charge fractions and follows updates", () => {
    const context = vm.createContext({ laptopBatteryAvailable: true, battery: { percentage: 0.8 }, UPower: { onBattery: true } });
    vm.runInContext(method, context);
    for (const [fraction, expected] of [[0.8, 80], [0.426, 43], [0.01, 1], [0, 0], [1, 100]]) {
        context.battery.percentage = fraction;
        assert.equal(context.batteryAccessibleName(), `Battery ${expected} percent, discharging`);
    }
    context.UPower.onBattery = false;
    assert.equal(context.batteryAccessibleName(), "Battery 100 percent, charging");
    context.laptopBatteryAvailable = false;
    assert.equal(context.batteryAccessibleName(), "Battery status unavailable");
});
