// Controlled receiver: no app data, network content, or accessibility launch flags.
const { app, BrowserWindow, clipboard, nativeImage } = require("electron");
const { execFile, spawn } = require("node:child_process");
const { promisify } = require("node:util");
const path = require("node:path");
const readline = require("node:readline");
const assert = require("node:assert/strict");
const exec = promisify(execFile);
const drive = (mode, query = "") =>
    exec("python3", [path.join(__dirname, "emoji-electron-input.py"), "--drive", mode, String(process.pid), query], {
        timeout: 10000,
    });
app.whenReady().then(async () => {
    const window = new BrowserWindow({ width: 700, height: 650, title: "Bingux Electron input test" });
    await window.loadURL(
        "data:text/html," +
            encodeURIComponent(
                '<body style="padding:80px 40px;font:24px sans-serif">' +
                    '<input id="input" style="font:inherit;width:400px"><textarea id="area" style="font:inherit;margin-top:70px"></textarea>' +
                    '<div id="edit" contenteditable style="margin-top:70px;min-height:32px"></div>',
            ),
    );
    const cases = [
        ["input", "Before ", 7, 7, "grinning face", "Before 😀"],
        ["input", "", 0, 0, "woman technologist", "👩‍💻"],
        ["area", "First\nSecond line", 9, 9, "thumbs up", "First\nSec👍🏽ond line"],
        ["edit", "Before replace after", 7, 14, "grinning face", "Before 😀 after"],
    ];
    try {
        for (const [id, text, start, end, query, expected] of cases) {
            await drive("focus");
            await window.webContents.executeJavaScript(`(() => {
                const e = document.getElementById(${JSON.stringify(id)});
                if (e.isContentEditable) {
                    e.textContent = ${JSON.stringify(text)};
                    e.focus();
                    const r = document.createRange();
                    r.setStart(e.firstChild, ${start}); r.setEnd(e.firstChild, ${end});
                    const s = getSelection(); s.removeAllRanges(); s.addRange(r);
                } else {
                    e.value = ${JSON.stringify(text)}; e.focus(); e.setSelectionRange(${start}, ${end});
                }
            })()`);
            clipboard.write({
                text: "Keep clipboard 😀",
                html: "<b>Keep rich text</b>",
                rtf: "{\\rtf1 Keep formatting}",
                image: nativeImage.createFromBitmap(Buffer.from([23, 45, 67, 255]), { width: 1, height: 1 }),
            });
            const saved = new Map(clipboard.availableFormats().map((format) => [format, clipboard.readBuffer(format)]));
            await drive("exercise", query);
            for (const [format, bytes] of saved)
                assert.deepEqual(clipboard.readBuffer(format), bytes, "Clipboard restored: " + format);
            const actual = await window.webContents.executeJavaScript(`(() => {
                const e = document.getElementById(${JSON.stringify(id)});
                return e.isContentEditable ? e.textContent : e.value;
            })()`);
            assert.equal(actual, expected);
            console.log(`PASS Electron ${id}: ${query}, Win+Period, Enter and clipboard preservation`);
        }
        if (process.env.GNOBLIN_TEST_XWAYLAND === "1") {
            const startHelper = async () => {
                const child = spawn(
                    "python3",
                    [path.join(process.env.GNOBLIN_SOURCE, "src/scripts/lib/clipboard-paste.py")],
                    { env: { ...process.env, GDK_BACKEND: "x11" } },
                );
                child.stdin.write(JSON.stringify("😀") + "\n");
                const records = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
                const next = async () => JSON.parse((await records.next()).value);
                assert.equal((await next()).event, "ready");
                return { child, next };
            };
            // Abort must restore all original formats, not just text.
            const saved = new Map(clipboard.availableFormats().map((format) => [format, clipboard.readBuffer(format)]));
            let helper = await startHelper();
            helper.child.stdin.end("ABORT\n");
            assert.equal((await helper.next()).event, "error");
            for (const [format, bytes] of saved) assert.deepEqual(clipboard.readBuffer(format), bytes);
            // A new user copy wins over the saved clipboard.
            helper = await startHelper();
            clipboard.writeText("New copy during paste");
            assert.equal((await helper.next()).event, "done");
            assert.equal(clipboard.readText(), "New copy during paste");
            // An empty clipboard must remain empty after cancellation.
            clipboard.clear();
            helper = await startHelper();
            helper.child.stdin.end("ABORT\n");
            assert.equal((await helper.next()).event, "error");
            assert.deepEqual(clipboard.availableFormats(), []);
            console.log("PASS clipboard image bytes, rich formats, cancellation, new copy and empty clipboard");
        }
        await window.webContents.executeJavaScript("document.activeElement.blur()");
        if (process.env.GNOBLIN_TEST_XWAYLAND !== "1") await drive("blur");
        if (process.env.GNOBLIN_TEST_XWAYLAND !== "1")
            console.log("PASS Electron input blur clears the caret and rejects insertion");
        app.exit(0);
    } catch (error) {
        console.error(error);
        app.exit(1);
    }
});
