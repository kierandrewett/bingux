import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import test from 'node:test';
const model = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shell/bingux/ControlLayout.js', import.meta.url), 'utf8'), model);
const plain = value => JSON.parse(JSON.stringify(value));
test('frontend and backend import exactly the same native groups', () => {
    const backend = fileURLToPath(new URL('../shell/bingux/settings-backend.py', import.meta.url));
    const output = execFileSync('python3', ['-c', 'import importlib.util,json,sys; s=importlib.util.spec_from_file_location("settings",sys.argv[1]); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); print(json.dumps(m.native_control_layout()))', backend], {encoding: 'utf8'});
    assert.deepEqual(plain(model.defaults()), JSON.parse(output));
});
test('reordering a group preserves its components and all other native groups', () => {
    const original = model.defaults();
    const moved = model.move(original, 'control-centre', 'control-media', 0);
    assert.equal(moved.groups['control-centre'][0], 'control-media');
    assert.deepEqual(plain(moved.groups['controls-header']), plain(original.groups['controls-header']));
    assert.equal(original.groups['control-centre'][0], 'controls-header');
    assert.equal(new Set(moved.groups['control-centre']).size, original.groups['control-centre'].length);
});
test('removal and restoration do not recreate the other groups', () => {
    const original = model.defaults();
    const removed = model.move(original, 'controls-header', 'control-settings', -1);
    assert.equal(model.contains(removed, 'controls-header', 'control-settings'), false);
    const restored = model.move(removed, 'controls-header', 'control-settings', 3);
    assert.deepEqual(plain(restored), plain(original));
    assert.equal(model.move(original, 'controls-audio', 'control-settings', 0), original);
});
test('invalid versions, groups and duplicate entries are rejected', () => {
    assert.equal(model.valid(model.defaults()), true);
    for (const mutate of [value => value.version = 2, value => value.version = true,
        value => value.groups['controls-audio'].push('control-volume'),
        value => value.groups['controls-header'].push('unknown'), value => value.groups.extra = [],
        value => delete value.groups['controls-audio']]) {
        const value = plain(model.defaults()); mutate(value); assert.equal(model.valid(value), false);
    }
});

test('every native group member has one palette entry and its original group', () => {
    const expected = Object.values(model.defaults().groups).flat();
    const widgets = vm.runInContext("widgets", model);
    assert.equal(widgets.length, expected.length);
    assert.equal(new Set(widgets.map(item => item.id)).size, expected.length);
    for (const [group, ids] of Object.entries(model.defaults().groups)) {
        for (const id of ids) {
            assert.ok(model.widget(id)?.label);
            assert.equal(model.groupFor(id), group);
        }
    }
});

test('action placement rejects duplicate ownership after loading settings', () => {
    const desktop = {controlLayout: model.defaults(), layout: {dock: ["control-settings"]}};
    assert.equal(model.validPlacement(desktop), false);
    desktop.controlLayout = model.move(desktop.controlLayout, "controls-header", "control-settings", -1);
    assert.equal(model.validPlacement(desktop), true);
    desktop.controlLayout = null;
    assert.equal(model.validPlacement(desktop), false);
});

test('audio and battery placement reject simultaneous native and external ownership', () => {
    for (const id of ['control-volume', 'control-microphone', 'control-battery']) {
        const desktop = {controlLayout: model.defaults(), layout: {'top-left': [id]}};
        assert.equal(model.validPlacement(desktop), false);
        desktop.controlLayout = model.move(desktop.controlLayout, model.groupFor(id), id, -1);
        assert.equal(model.validPlacement(desktop), true);
    }
});
