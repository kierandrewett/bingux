import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";

const source = readFileSync(new URL("../shell/bingux/SearchResult.qml", import.meta.url), "utf8");
function highlight(text, query) {
    const context = vm.createContext({ root: { query }, Theme: { searchAccent: "#a9c8f7" } });
    vm.runInContext(
        source.slice(source.indexOf("    function escapeRichText"), source.indexOf("    function defaultIcon")),
        context,
    );
    return context.highlightedText(text);
}

test("provider markup is escaped before match styling", () => {
    const result = highlight('<img src="remote">&Firefox', "fire");
    assert.ok(!result.includes("<img"));
    assert.ok(result.includes("&lt;img"));
    assert.ok(result.includes("&amp;"));
    assert.ok(result.includes("<b>Fire</b>"));
});
test("all occurrences and separate query terms are emphasized", () => {
    const result = highlight("Note note archive", "note archive");
    assert.equal((result.match(/<b>/g) || []).length, 3);
});
test("ordered fuzzy matches are emphasized only when the full term matches", () => {
    assert.equal((highlight("Firefox", "ffx").match(/<b>/g) || []).length, 3);
    assert.equal(highlight("Firefox", "fxz"), "Firefox");
});
test("empty query preserves escaped text and literal regex punctuation", () => {
    assert.equal(highlight("A & B", ""), "A &amp; B");
    assert.ok(highlight("a+b", "+").includes("<b>+</b>"));
});

function compact(path, query, width) {
    const context = vm.createContext({ root: { query }, Theme: { searchAccent: "#a9c8f7" } });
    vm.runInContext(
        source.slice(source.indexOf("    function escapeRichText"), source.indexOf("    function defaultIcon")),
        context,
    );
    return context.compactPath(path, width, (rich) => rich.replace(/<[^>]*>/g, "").length);
}
const plain = (rich) => rich.replace(/<[^>]*>/g, "");
test("query syntax highlights only positive terms and exact phrases", () => {
    const result = highlight("report draft OR pdf", "report OR notes -draft filetype:pdf");
    assert.ok(result.includes("<b>report</b>"));
    assert.ok(!result.includes("<b>draft</b>"));
    assert.ok(!result.includes("<b>OR</b>"));
    assert.ok(!result.includes("<b>pdf</b>"));
    assert.ok(highlight("Visual Studio Code", '"Visual Studio"').includes("<b>Visual Studio</b>"));
    assert.equal(highlight("Visual Studio Code", '"visual code"'), "Visual Studio Code");
});
test("short paths are unchanged and long paths preserve the filename", () => {
    assert.equal(compact("/home/me/report.pdf", "", 50), "/home/me/report.pdf");
    const result = plain(compact("/home/me/Documents/old/project/archive/report.pdf", "", 25));
    assert.ok(result.length <= 25);
    assert.ok(result.includes("…"));
    assert.ok(result.endsWith("/report.pdf"));
});
test("directory matches survive shortening alongside the filename", () => {
    const result = compact("/home/me/Documents/needle/archive/very-long-folder/report.pdf", "needle", 30);
    assert.ok(result.includes("<b>needle</b>"));
    assert.ok(plain(result).endsWith("report.pdf"));
    assert.ok(plain(result).length <= 30);
});
test("separated and fuzzy matches survive even when the viewport is too narrow", () => {
    for (const query of ["needle report", "ndlrpt"]) {
        const path = "/home/me/needle/archive/report.pdf";
        const matches = (rich) => [...rich.matchAll(/<b>(.*?)<\/b>/g)].map((match) => match[1]).join("");
        assert.equal(matches(compact(path, query, 4)), matches(highlight(path, query)));
    }
});
test("compacted paths escape provider markup and never split emoji", () => {
    const result = compact("/home/😀😀😀/<script>/report.pdf", "", 20);
    assert.ok(!result.includes("<script>"));
    assert.ok(plain(result).isWellFormed());
});
