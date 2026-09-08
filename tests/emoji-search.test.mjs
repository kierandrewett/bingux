import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const api = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shell/bingux/EmojiSearch.js', import.meta.url), 'utf8'), api);
const rows = JSON.parse(readFileSync(new URL('../shell/bingux/emoji-data.json', import.meta.url))).emoji;
test('catalogue preserves Unicode sequences and includes all major categories', () => {
    assert.equal(rows.length, 3944);
    assert.ok(rows.some(row => row.emoji.includes('\u200d')));
    assert.ok(rows.some(row => row.name.includes('skin tone')));
    assert.equal(new Set(rows.map(row => row.emoji)).size, rows.length);
});
test('search is case insensitive, multiword, alias aware and ranks exact names first', () => {
    assert.equal(api.search(rows, 'THUMBS UP', '', [])[0].name, 'thumbs up');
    assert.ok(api.search(rows, ':lol:', '', []).length > 0);
    assert.ok(api.search(rows, 'cat', 'Animals & Nature', []).every(row => row.group === 'Animals & Nature'));
    assert.equal(api.search(rows, 'notarealemojiword', '', []).length, 0);
});
test('recents deduplicate, preserve order and are bounded', () => {
    assert.deepEqual(Array.from(api.remember(['😀', '❤️'], '❤️')), ['❤️', '😀']);
    assert.equal(api.remember(Array.from({length: 40}, (_, i) => String(i)), '😀').length, 32);
    assert.equal(api.search(rows, '', 'recent', ['missing', '😀']).length, 1);
});
test('skin tones collapse to one entry and a global preference selects valid sequences', () => {
    const folded = api.foldSkinTones(rows);
    assert.equal(folded.length, 1914);
    assert.equal(new Set(folded.map(row => row.name)).size, folded.length);
    for (let tone = 0; tone < 6; tone++) {
        const displayed = api.search(folded, '', '', [], tone);
        assert.equal(displayed.length, folded.length);
        assert.ok(displayed.every(row => rows.some(original => original.emoji === row.emoji)));
        const hands = api.search(folded, 'thumbs up', '', [], tone);
        assert.equal(hands.length, 1);
        assert.equal(hands[0].emoji, tone ? '👍' + String.fromCodePoint(0x1f3fa + tone) : '👍');
    }
    assert.equal(api.search(folded, 'woman technologist', '', [], 3)[0].emoji, '👩🏽‍💻');
    assert.equal(api.search(folded, 'handshake', '', [], 3).length, 1);
    const recent = api.search(folded, '', 'recent', ['👍🏻', '👍🏿', '😀'], 3);
    assert.deepEqual(Array.from(recent, row => row.emoji), ['👍🏽', '😀']);
});
