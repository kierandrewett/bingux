import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

function fixture() {
    const source = readFileSync(new URL('../shell/bingux/NotificationState.qml', import.meta.url), 'utf8');
    let now = 1000;
    const history = {};
    vm.runInNewContext(readFileSync(new URL('../shell/bingux/NotificationHistory.js', import.meta.url), 'utf8'), history);
    const context = vm.createContext({
        History: history, historyReady: false,
        Date: { now: () => now },
        DesktopEntries: { applications: {values: []}, byId: id => id === 'org.example.Files' ? { name: 'Files', icon: 'files-icon' } : null },
        expiryTimer: { stop() {}, restart() {}, interval: 0 },
        retainedState: { receivedTimesJson: "{}", hiddenIdsJson: "{}" },
    });
    // Execute the production state functions with deterministic time and service doubles.
    const properties = [...source.matchAll(/(?:readonly )?property (?:int|var) (\w+): (.+)/g)]
        .filter(([, name]) => name !== "visibleEntries")
        .map(([, name, value]) => `var ${name} = ${value};`).join('\n');
    vm.runInContext(`${properties}\nvar root = this;\n${source.slice(source.indexOf('    function boundedText'), source.indexOf('    expiryTimer: Timer'))}`, context);
    Object.defineProperty(context, 'visibleEntries', { get() { return context.allEntries.filter(entry => entry.toastVisible); } });
    let nextId = 1;
    const make = (timeout = 5) => ({
        id: nextId++, expireTimeout: timeout, appName: 'Sender', appIcon: '', desktopEntry: '',
        summary: 'Ready', body: 'Done', actions: [],
        closed: { connect() {}, disconnect() {} },
        expire() { this.expired = true; }, dismiss() { this.dismissed = true; },
    });
    return { state: context, make, advance(ms) { now += ms; context.expireDueNotifications(); } };
}

test('hover preserves remaining time and resumes on leave', () => {
    const { state, make, advance } = fixture();
    const notification = make();
    state.accept(notification);
    advance(2000);
    state.setPaused(notification, true);
    advance(20000);
    assert.equal(notification.expired, undefined);
    state.setPaused(notification, false);
    advance(2999);
    assert.equal(notification.expired, undefined);
    advance(1);
    assert.equal(state.visibleEntries.length, 0);
    assert.equal(state.allEntries.length, 1);
    assert.equal(notification.expired, undefined);
});

test('replacement and content updates stay paused', () => {
    const { state, make, advance } = fixture();
    const notification = make();
    state.accept(notification);
    state.setPaused(notification, true);
    state.resetExpiry(notification);
    advance(20000);
    assert.equal(notification.expired, undefined);
    const replacement = make(2);
    replacement.id = notification.id;
    state.accept(replacement);
    advance(20000);
    assert.equal(replacement.expired, undefined);
    state.setPaused(replacement, false);
    advance(4000);
    assert.equal(state.visibleEntries.length, 0);
    assert.equal(state.allEntries[0].notification, replacement);
});

test('persistent notifications do not gain an expiry after hover', () => {
    const { state, make, advance } = fixture();
    const notification = make(0);
    state.accept(notification);
    state.setPaused(notification, true);
    state.setPaused(notification, false);
    advance(100000);
    assert.equal(notification.expired, undefined);
});

test('desktop metadata supplies app name and missing icon with sender fallback', () => {
    const { state, make } = fixture();
    const notification = make();
    notification.desktopEntry = 'org.example.Files.desktop';
    let entry = state.entryFor(notification, 0);
    assert.equal(entry.appName, 'Files');
    assert.equal(entry.appIcon, 'files-icon');
    notification.desktopEntry = 'missing';
    notification.appIcon = '/tmp/icon.png';
    entry = state.entryFor(notification, 0);
    assert.equal(entry.appName, 'Sender');
    assert.equal(entry.appIcon, '/tmp/icon.png');
});

test('dismissal leaves other cards in the stack without expiring paused cards', () => {
    const { state, make, advance } = fixture();
    const notifications = Array.from({ length: 4 }, () => make(0));
    notifications.forEach(notification => state.accept(notification));
    state.setPaused(notifications[1], true);
    state.dismiss(notifications[0]);
    assert.equal(notifications[0].dismissed, true);
    assert.equal(state.visibleEntries.length, 3);
    advance(10000);
    assert.equal(state.visibleEntries.length, 3);
});

test('all notifications remain in the newest-first stack, without a three-card cap', () => {
    const { state, make } = fixture();
    const notifications = Array.from({ length: 36 }, () => make(0));
    notifications.forEach(notification => state.accept(notification));
    assert.equal(state.visibleEntries.length, 36);
    assert.deepEqual(Array.from(state.visibleEntries, entry => entry.notification.id), notifications.map(notification => notification.id).reverse());
    assert.equal(notifications[0].expired, undefined);
    state.dismiss(notifications[35]);
    assert.equal(state.visibleEntries.length, 35);
    assert.equal(state.visibleEntries[0].notification.id, 35);
    assert.equal(state.visibleEntries[34].notification.id, 1);
});

test('received time survives replay and is removed on dismissal', () => {
    const { state, make, advance } = fixture();
    const notification = make(0);
    state.accept(notification);
    const receivedAt = state.visibleEntries[0].receivedAt;
    advance(5000);
    notification.lastGeneration = true;
    state.accept(notification);
    assert.equal(state.visibleEntries[0].receivedAt, receivedAt);
    state.dismiss(notification);
    assert.equal(JSON.parse(state.retainedState.receivedTimesJson)[String(notification.id)], undefined);
});

test('timeout progress fills, freezes on hover, and resumes from the same value', () => {
    const { state, make, advance } = fixture();
    const notification = make(5);
    state.accept(notification);
    assert.equal(state.expiryProgress(notification), 0);
    advance(2000);
    assert.equal(state.expiryProgress(notification), 0.4);
    state.setPaused(notification, true);
    advance(20000);
    assert.equal(state.expiryProgress(notification), 0.4);
    state.setPaused(notification, false);
    advance(2500);
    assert.equal(state.expiryProgress(notification), 0.9);
    advance(500);
    assert.equal(state.expiryProgress(notification), 1);
});


test('timed-out notifications remain tracked and retain actions until cleared', () => {
    const { state, make, advance } = fixture();
    const notification = make(-1);
    state.accept(notification);
    advance(4000);
    assert.equal(state.visibleEntries.length, 0);
    assert.equal(state.allEntries.length, 1);
    assert.equal(notification.tracked, true);
    assert.equal(notification.expired, undefined);
    assert.equal(state.notificationWatchers.length, 1);
    state.dismiss(notification);
    assert.equal(state.allEntries.length, 0);
    assert.equal(notification.dismissed, true);
});

test('replacing an archived notification resurfaces it without increasing the count', () => {
    const { state, make, advance } = fixture();
    const notification = make(-1);
    state.accept(notification);
    advance(4000);
    const replacement = make(-1);
    replacement.id = notification.id;
    state.accept(replacement);
    assert.equal(state.allEntries.length, 1);
    assert.equal(state.visibleEntries[0].notification, replacement);
    assert.equal(state.notificationWatchers.length, 1);
});

test('reload restores hidden notifications without replaying their toasts', () => {
    const { state, make, advance } = fixture();
    const notification = make(-1);
    state.accept(notification);
    advance(4000);
    const replay = make(-1);
    replay.id = notification.id;
    replay.lastGeneration = true;
    state.allEntries = [];
    state.accept(replay);
    assert.equal(state.allEntries.length, 1);
    assert.equal(state.visibleEntries.length, 0);
    state.dismissAll();
    assert.equal(state.allEntries.length, 0);
    assert.equal(replay.dismissed, true);
});


test('Electron notification identity resolves an installed variant and its application icon', () => {
    const { state, make } = fixture();
    state.DesktopEntries.applications.values = [{id: 'discord-canary', startupClass: 'discord', name: 'Discord Canary', icon: '/opt/discord-canary/discord.png'}];
    const notification = make();
    Object.assign(notification, {desktopEntry: 'discord', appName: 'Discord', appIcon: 'discord'});
    const entry = state.entryFor(notification, 0);
    assert.equal(entry.appIcon, '/opt/discord-canary/discord.png');
    notification.desktopEntry = '';
    assert.equal(state.entryFor(notification, 0).appIcon, entry.appIcon);
    notification.appName = 'Discord lookalike';
    assert.equal(state.applicationFor(notification), null);
});


test('reading time includes title and body, with minimum and maximum durations', () => {
    const { state, make } = fixture();
    const notification = make(-1);
    assert.equal(state.timeoutFor(notification), 4000);
    notification.body = 'word '.repeat(20).trim();
    assert.equal(state.timeoutFor(notification), 7800);
    const bodyTime = state.timeoutFor(notification);
    notification.summary = notification.body;
    notification.body = 'Ready';
    assert.equal(state.timeoutFor(notification), bodyTime);
    notification.body = 'word '.repeat(1000);
    assert.equal(state.timeoutFor(notification), 20000);
});

test('reading time handles unbroken text and respects persistent requests', () => {
    const { state, make } = fixture();
    const notification = make(1);
    notification.body = '文'.repeat(120);
    assert.ok(state.timeoutFor(notification) > 8000);
    notification.expireTimeout = 0;
    assert.equal(state.timeoutFor(notification), 0);
    notification.expireTimeout = 12;
    notification.body = 'Done';
    assert.equal(state.timeoutFor(notification), 12000);
    notification.expireTimeout = 120;
    assert.equal(state.timeoutFor(notification), 20000);
});

test('longer content refreshes the reading budget while paused', () => {
    const { state, make, advance } = fixture();
    const notification = make(-1);
    state.accept(notification);
    advance(1000);
    state.setPaused(notification, true);
    notification.body = 'word '.repeat(20).trim();
    state.resetExpiry(notification);
    advance(30000);
    assert.equal(state.visibleEntries.length, 1);
    state.setPaused(notification, false);
    advance(7799);
    assert.equal(state.visibleEntries.length, 1);
    advance(1);
    assert.equal(state.visibleEntries.length, 0);
});

test('notification file icons retain their source for alpha-preserving SVG rendering', () => {
    const { state } = fixture();
    assert.equal(state.notificationImage({image: 'image://notification/1', hints: {'image-path': '/icons/mouse.svg'}}), '/icons/mouse.svg');
    assert.equal(state.notificationImage({image: 'image://notification/2', hints: {'image-path': '/icons/mouse.svg', 'image-data': [40, 40]}}), 'image://notification/2');
    assert.equal(state.notificationImage({image: 'image://notification/3'}), 'image://notification/3');
});

test('notification clicks prefer sender actions over app fallback', () => {
    const { state } = fixture();
    let invoked = 0;
    const entry = {actions: [{defaultAction: true, action: {invoke() { invoked++; }}}]};
    assert.equal(state.canActivate(entry), true);
    assert.equal(state.activate(entry), true);
    assert.equal(invoked, 1);
});

test('notification clicks focus matching windows or launch their desktop entry', () => {
    const { state, make } = fixture();
    const app = {id: 'org.example.Editor', name: 'Editor', startupClass: 'editor'};
    state.DesktopEntries.applications.values = [app];
    let focused = 0;
    state.ToplevelManager = {toplevels: {values: [{appId: 'editor', activate() { focused++; }}]}};
    const notification = make();
    notification.desktopEntry = app.id;
    state.accept(notification);
    const entry = state.allEntries[0];
    assert.equal(state.canActivate(entry), true);
    assert.equal(state.activate(entry), true);
    assert.equal(focused, 1);
    assert.equal(state.visibleEntries.length, 0);
    state.ToplevelManager.toplevels.values = [];
    let command;
    state.Quickshell = {env: () => '/test/launcher', execDetached: args => {command = Array.from(args);}};
    assert.equal(state.activate(entry), true);
    assert.deepEqual(command, ['/test/launcher', '--notify-errors', '--', app.id]);
    const unknown = {desktopEntry: '', appName: 'Unknown', actions: []};
    assert.equal(state.canActivate(unknown), false);
    assert.equal(state.activate(unknown), false);
});
