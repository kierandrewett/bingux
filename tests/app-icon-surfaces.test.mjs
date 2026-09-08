import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import test from 'node:test';

const directory = new URL('../shell/bingux/', import.meta.url);

test('full-colour shell icons cannot bypass the shared OS renderer', () => {
    const nativePrimitives = new Set(['OsIconImage.qml', 'SymbolicIcon.qml']);
    for (const file of readdirSync(directory)) {
        if (!file.endsWith('.qml') || file.endsWith('Test.qml') || nativePrimitives.has(file)) continue;
        assert.doesNotMatch(readFileSync(new URL(file, directory), 'utf8'), /\bIconImage\s*\{/, `${file} must use OsIconImage for application icons`);
    }
});

test('dock, menus, notifications, tray and launch copies use OS icons', () => {
    for (const file of ['Dock.qml', 'NotificationStack.qml', 'Tray.qml', 'SearchResult.qml', 'SearchLaunchEffect.qml']) {
        assert.match(readFileSync(new URL(file, directory), 'utf8'), /\bOsIconImage\s*\{/, file);
    }
});
