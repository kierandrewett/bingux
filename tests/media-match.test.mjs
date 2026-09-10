import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const api = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shell/bingux/MediaMatch.js', import.meta.url), 'utf8'), api);
const group = { id: 'Spotify', desktopEntry: { id: 'spotify.desktop' }, windows: [] };
test('indexed notifications preserve exact matching, order and repeated entries', () => {
    const shared = {appName: 'Spotify'};
    const entries = [null, shared, {desktopEntry: 'other', appName: 'Spotify'},
        {desktopEntry: 'spotify.desktop'}, {appName: 'Music Player'}, shared,
        {appName: 'window-alias'}, {desktopEntry: 'startup'}, {appName: '.desktop'},
        {desktopEntry: '.desktop', appName: 'Spotify'}, {}, {appName: 'spotify-advert'}];
    const groups = [null, {}, group, {id: 'other'}, {id: 'spotify',
        desktopEntry: {id: 'spotify.desktop', name: 'Music Player', startupClass: 'startup'},
        windows: [{appId: 'window-alias'}, {appId: 'spotify'}]}];
    const index = api.notificationIndex(entries);
    for (const candidate of groups) {
        const expected = entries.filter(entry => api.matchesNotification(entry, candidate));
        assert.deepEqual(Array.from(api.notificationsForGroup(index, candidate)), expected);
    }
    assert.deepEqual(Array.from(api.notificationsForGroup(api.notificationIndex([]), group)), []);
});
test('group lookups do not re-read unrelated notification identities', () => {
    let reads = 0;
    const entries = Array.from({length: 1000}, (_, i) => ({get desktopEntry() { reads++; return 'app' + i; }}));
    const index = api.notificationIndex(entries);
    reads = 0;
    for (let i = 0; i < 100; i++) {
        assert.equal(api.notificationsForGroup(index, {id: 'app' + i})[0], entries[i]);
    }
    assert.equal(reads, 0);
});
test('player icons resolve through the OS provider, including browser identities and packaged IDs', () => {
    const spotify = {id: 'com.spotify.Client', icon: 'com.spotify.Client'};
    const helium = {id: 'helium', icon: 'helium'};
    const provider = {
        byId: id => id === 'helium' ? helium : null,
        heuristicLookup: id => ({spotify, Helium: helium})[id] || null,
    };
    assert.equal(api.playerDesktopEntry({desktopEntry: 'spotify.desktop'}, provider), spotify);
    assert.equal(api.playerDesktopEntry({identity: 'Helium', dbusName: 'org.mpris.MediaPlayer2.chromium.instance42'}, provider), helium);
    assert.equal(api.playerDesktopEntry({dbusName: 'org.mpris.MediaPlayer2.helium.instance42'}, provider), helium);
    assert.equal(api.playerDesktopEntry({desktopEntry: 'unknown', identity: 'Helium'}, provider), null);
    assert.equal(api.playerDesktopEntry(null, provider), null);
});
test('matches the desktop entry, startup class and window app IDs', () => {
    assert.equal(api.matches({ desktopEntry: 'spotify' }, group), true);
    assert.equal(api.matches({ desktopEntry: 'org.videolan.VLC' }, { id: 'vlc', desktopEntry: { id: 'org.videolan.VLC.desktop' } }), true);
    assert.equal(api.matches({ desktopEntry: 'browser' }, { windows: [{ appId: 'browser' }] }), true);
    assert.equal(api.matches({ desktopEntry: 'Foo' }, { desktopEntry: { startupClass: 'Foo' } }), true);
});
test('never guesses from track titles or player names, nor overrides a desktop entry', () => {
    assert.equal(api.matches({ desktopEntry: 'browser', dbusName: 'org.mpris.MediaPlayer2.spotify' }, group), false);
    assert.equal(api.matches({ identity: 'Spotify', trackTitle: 'Spotify' }, group), false);
    assert.equal(api.matches({ desktopEntry: 'spotify-advert' }, group), false);
    assert.equal(api.matches(null, group), false);
    assert.equal(api.matches({}, null), false);
});
test('uses the exact bus identity only when the desktop entry is missing', () => {
    assert.equal(api.matches({ dbusName: 'org.mpris.MediaPlayer2.spotify' }, group), true);
    assert.equal(api.matches({ dbusName: 'org.mpris.MediaPlayer2.vlc.instance123' }, { id: 'vlc' }), true);
    assert.equal(api.matches({ dbusName: 'org.mpris.MediaPlayer2.vlc.instance123' }, group), false);
});
test('formats media durations without invalid or negative times', () => {
    assert.equal(api.timeLabel(227.076), '3:47');
    assert.equal(api.timeLabel(-1), '0:00');
    assert.equal(api.timeLabel(NaN), '0:00');
});
test('audio identity outranks generic runtime names and excludes other apps', () => {
    assert.equal(api.matchesAudio({ 'application.name': 'Spotify' }, group), true);
    assert.equal(api.matchesAudio({ 'application.id': 'other', 'application.name': 'Spotify' }, group), false);
    assert.equal(api.matchesAudio({ 'application.process.binary': '/usr/bin/pikvm-desktop', 'application.name': 'Chromium' }, { id: 'pikvm-desktop' }), true);
    assert.equal(api.matchesAudio({ 'application.process.binary': 'pikvm-desktop', 'application.name': 'Chromium' }, { id: 'chromium' }), false);
    assert.equal(api.matchesAudio({ 'application.name': 'Some Browser' }, { desktopEntry: { name: 'Some Browser' } }), true);
    assert.equal(api.matchesAudio({}, group), false);
});
test('notification desktop identities are authoritative with exact legacy-name fallback', () => {
    assert.equal(api.matchesNotification({ desktopEntry: 'spotify.desktop' }, group), true);
    assert.equal(api.matchesNotification({ desktopEntry: 'other', appName: 'Spotify' }, group), false);
    assert.equal(api.matchesNotification({ appName: 'Spotify' }, group), true);
    assert.equal(api.matchesNotification({ appName: 'Spotify update available' }, group), false);
});
test('Chromium forks without DesktopEntry use their exact application identity', () => {
    const helium = {id: 'helium', desktopEntry: {id: 'helium', name: 'Helium'}, windows: [{appId: 'helium'}]};
    const player = {identity: 'Helium', dbusName: 'org.mpris.MediaPlayer2.chromium.instance4006326'};
    assert.equal(api.matches(player, helium), true);
    assert.equal(api.matches(player, {id: 'chromium', desktopEntry: {name: 'Chromium'}}), false);
    assert.equal(api.matches(player, group), false);
    assert.equal(api.matches({...player, desktopEntry: 'other'}, helium), false);
    assert.equal(api.matches({...player, identity: 'Helium video'}, helium), false);
});
test('long items and video providers seek; short music keeps track controls', () => {
    assert.equal(api.prefersSeeking({lengthSupported: true, length: 600}), false);
    assert.equal(api.prefersSeeking({lengthSupported: true, length: 601}), true);
    assert.equal(api.prefersSeeking({lengthSupported: false, length: 900}), false);
    assert.equal(api.prefersSeeking({metadata: {'xesam:url': 'https://www.youtube.com/watch?v=abc'}}), true);
    assert.equal(api.prefersSeeking({metadata: {'xesam:url': 'https://music.youtube.com/watch?v=abc'}}), false);
    assert.equal(api.prefersSeeking({metadata: {'xesam:url': 'https://youtube.com.evil.invalid/watch'}}), false);
    assert.equal(api.prefersSeeking({metadata: {'xesam:contentType': 'video/mp4'}}), true);
    assert.equal(api.prefersSeeking({desktopEntry: 'org.gnome.Showtime'}), true);
    assert.equal(api.prefersSeeking({identity: 'Helium'}), false);
});
test('seek steps respect capabilities and track boundaries', () => {
    const calls = [];
    const player = {canControl: true, canSeek: true, lengthSupported: true, length: 900,
        positionSupported: true, position: 5, seek: value => calls.push(value)};
    api.step(player, -1);
    player.position = 895;
    api.step(player, 1);
    player.position = 100;
    api.step(player, -1);
    api.step(player, 1);
    assert.deepEqual(calls, [-5, 5, -10, 10]);
    player.canSeek = false;
    assert.equal(api.canStep(player, 1), false);
    api.step(player, 1);
    assert.equal(calls.length, 4);
    player.length = 180; player.canGoNext = true; player.next = () => calls.push('next');
    api.step(player, 1);
    assert.equal(calls.at(-1), 'next');
    player.canControl = false;
    assert.equal(api.canStep(player, 1), false);
});
