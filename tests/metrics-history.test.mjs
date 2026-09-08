import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';
const history = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shell/bingux/MetricsHistory.js', import.meta.url), 'utf8'), history);
const record = {cpuPercent: 0, memoryUsedBytes: 25, memoryTotalBytes: 100, networkReceiveBytesPerSecond: null, networkTransmitBytesPerSecond: 0};
test('history retains five minutes with bounded storage and preserves missing readings', () => {
    let points = [];
    for (let n = 0; n < 900; n++) points = history.append(points, record, n * 1000);
    assert.equal(points.length, 300);
    assert.equal(points[0].at, 600000);
    assert.equal(points.at(-1).cpu, 0);
    assert.equal(points.at(-1).memory, 25);
    assert.equal(points.at(-1).receive, null);
    assert.equal(points.at(-1).send, 0);
    points = history.append(points, record, 899000);
    assert.equal(points.filter(point => point.at === 899000).length, 1);
});
test('outages and null readings break traces without losing real zero usage', () => {
    const points = [{at: 1000, cpu: 10}, {at: 2000, cpu: 0}, {at: 3000, cpu: null}, {at: 4000, cpu: 30}, {at: 10000, cpu: 50}];
    assert.deepEqual(Array.from(history.segments(points, 'cpu'), segment => Array.from(segment, point => point.cpu)), [[10, 0], [30], [50]]);
    assert.equal(history.nearest(points, 7000), null);
    assert.equal(history.nearest(points, 4500).cpu, 30);
    assert.equal(history.nearest(points, 15000), null);
});
test('time windows and network axes use the visible measured values', () => {
    const points = [{at: 1000, receive: 99999999}, {at: 60000, receive: 0, send: 0}, {at: 61000, receive: 2048, send: 4096}];
    const window = history.windowPoints(points, 62000, 10000);
    assert.equal(window.length, 2);
    assert.equal(history.rateMaximum(window, ['receive', 'send']), 4096);
    assert.equal(history.rateMaximum([], ['receive']), 1024);
    assert.equal(history.peak(window, ['receive']), 2048);
});
test('optional hardware samples preserve units and missing-sensor gaps', () => {
    const old = history.append([], record, 1000)[0];
    assert.equal(old.temperature, null);
    const point = history.append([], {...record, extra: {cpuTemperatureCelsius: 54.5, load1: 2.25, swapUsedBytes: 1024, swapTotalBytes: 4096, diskReadBytesPerSecond: 500, diskWriteBytesPerSecond: null}}, 2000)[0];
    assert.equal(point.temperature, 54.5);
    assert.equal(point.load, 2.25);
    assert.equal(point.swap, 25);
    assert.equal(point.swapUsed, 1024);
    assert.equal(point.diskRead, 500);
    assert.equal(point.diskWrite, null);
});
test('per-core history keeps CPU identities and leaves missing cores as gaps', () => {
    let points = history.append([], {...record, extra: {cpuCores: [{id: 0, usage: 100}, {id: 3, usage: 0}]}}, 1000);
    points = history.append(points, {...record, extra: {cpuCores: [{id: 0, usage: null}]}}, 2000);
    assert.equal(points[0].cpu0, 100);
    assert.equal(points[0].cpu3, 0);
    assert.equal(points[1].cpu0, null);
    assert.equal(points[1].cpu3, undefined);
    assert.equal(history.segments(points, 'cpu0')[0].length, 1);
});
