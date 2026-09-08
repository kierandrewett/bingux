import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const context = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shell/bingux/BrowserWindow.js', import.meta.url), 'utf8'), context);
const choose = context.choose;

test('focuses the search window only within the configured default browser', () => {
    const unrelated = { appId: 'other', title: 'cats', activated: true };
    const old = { appId: 'browser', title: 'Old tab' };
    const searched = { appId: 'browser', title: 'Cats at DuckDuckGo' };
    assert.equal(choose([unrelated, old, searched], 'browser.desktop', '', 'cats', false), searched);
    assert.equal(choose([unrelated], 'browser.desktop', '', 'cats', true), null);
});

test('supports desktop startup classes and waits for the search tab before falling back', () => {
    const window = { appId: 'BrowserClass', title: 'Old tab', activated: true };
    assert.equal(choose([window], 'org.example.Browser.desktop', 'browserclass', 'cats', false), null);
    assert.equal(choose([window], 'org.example.Browser.desktop', 'browserclass', 'cats', true), window);
    assert.equal(choose([window], '', '', 'cats', true), null);
});
