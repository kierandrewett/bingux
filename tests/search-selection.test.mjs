import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

test('input hint completes prefixes and labels other matches without altering the query', () => {
    const source = readFileSync(new URL('../shell/bingux/SearchOverlay.qml', import.meta.url), 'utf8');
    const context = vm.createContext({});
    vm.runInContext(source.slice(source.indexOf('    function inputHint('), source.indexOf('    function chatCandidate(')), context);
    assert.equal(context.inputHint('fi', 'Files'), 'les');
    assert.equal(context.inputHint('FILES', 'Files'), '');
    assert.equal(context.inputHint('fox', 'Firefox'), '  ·  Firefox');
    assert.equal(context.inputHint('', 'Files'), '');
    assert.equal(context.inputHint('fi', ''), '');
});

test('mouse movement and keyboard share selection without a stationary pointer stealing it', () => {
    const source = readFileSync(new URL('../shell/bingux/SearchOverlay.qml', import.meta.url), 'utf8');
    const root = { selectedIndex: 0, keyboardSelection: true, pointerBlocked: true, pointerResultIndex: -1, contentItem: {} };
    const context = vm.createContext({ Qt: { point: (x, y) => ({ x, y }) }, root, hovered: true, point: { position: { x: 10, y: 60 } }, lastPosition: { x: 9, y: 60 }, resultsList: { width: 660, height: 420, mapFromItem: (_, x, y) => ({ x, y }), contentX: 0, contentY: 0, indexAt: (x, y) => Math.floor(y / 50) } });
    vm.runInContext(source.slice(source.indexOf('                    function selectAtPointer()'), source.indexOf('                    onPointChanged:')), context);
    context.selectAtPointer();
    assert.equal(root.selectedIndex, 0, 'growth blocks pointer selection');
    root.pointerBlocked = false;
    context.selectAtPointer();
    assert.equal(root.selectedIndex, 0, 'growth completion does not select beneath a stationary cursor');
    assert.equal(root.pointerResultIndex, -1, 'stationary cursor cannot activate a row');
    context.point.position = { x: 10.5, y: 60 };
    context.selectAtPointer();
    assert.equal(root.pointerResultIndex, 1);
    assert.equal(root.selectedIndex, 1);
    assert.equal(root.keyboardSelection, false);
    root.selectedIndex = 2;
    root.keyboardSelection = true;
    context.selectAtPointer();
    assert.equal(root.selectedIndex, 2);
    assert.equal(root.keyboardSelection, true);
    context.point.position = { x: 11, y: 60 };
    context.selectAtPointer();
    assert.equal(root.selectedIndex, 1);
    context.hovered = false;
    context.selectAtPointer();
    assert.equal(root.selectedIndex, 1);
    const row = readFileSync(new URL('../shell/bingux/SearchResult.qml', import.meta.url), 'utf8');
    assert.match(row, /color: root.selected \? Theme.searchSelection : "transparent"/);
});

test('chevron entrance is claimed once per highlighted index, not per query or delegate', () => {
    const source = readFileSync(new URL('../shell/bingux/SearchOverlay.qml', import.meta.url), 'utf8');
    const context = vm.createContext({ lastChevronIndex: -1 });
    vm.runInContext(source.slice(source.indexOf('    function claimChevronAnimation'), source.indexOf('    property bool keyboardSelection')), context);
    assert.equal(context.claimChevronAnimation(0), true);
    for (let update = 0; update < 10; update++) assert.equal(context.claimChevronAnimation(0), false);
    assert.equal(context.claimChevronAnimation(1), true);
    assert.equal(context.claimChevronAnimation(1), false);
    assert.equal(context.claimChevronAnimation(0), true);
    assert.equal(context.claimChevronAnimation(-1), false);
    assert.equal(context.claimChevronAnimation(0), false);
});
