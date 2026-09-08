import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const api = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shell/bingux/MediaMatch.js', import.meta.url), 'utf8'), api);
const group = { id: 'Spotify', desktopEntry: { id: 'spotify.desktop' }, windows: [] };
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
