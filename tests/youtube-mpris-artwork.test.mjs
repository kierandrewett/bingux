import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

test('fills missing artwork, follows the playing video and preserves provider artwork', () => {
    let update;
    let videoId = 'hE1i8qQvGF8';
    const session = {metadata: {artwork: []}};
    const context = vm.createContext({
        navigator: {mediaSession: session},
        document: {getElementById: () => ({getVideoData: () => ({video_id: videoId})}), addEventListener() {}},
        setInterval: callback => { update = callback; },
    });
    vm.runInContext(readFileSync(new URL('../browser/youtube-mpris-artwork.user.js', import.meta.url), 'utf8'), context);
    assert.equal(session.metadata.artwork[0].src, 'https://i.ytimg.com/vi/hE1i8qQvGF8/hqdefault.jpg');
    videoId = 'abcdefghijk'; update();
    assert.equal(session.metadata.artwork[0].src, 'https://i.ytimg.com/vi/abcdefghijk/hqdefault.jpg');
    session.metadata = {artwork: []}; update();
    assert.equal(session.metadata.artwork.length, 1);
    session.metadata.artwork = [{src: 'https://example.org/provider-art.jpg'}]; update();
    assert.equal(session.metadata.artwork[0].src, 'https://example.org/provider-art.jpg');
    session.metadata = {artwork: []}; videoId = 'invalid/path'; update();
    assert.equal(session.metadata.artwork.length, 0);
    session.metadata = null; update();
});
