// Controlled receiver: no app data, network content, or accessibility launch flags.
const {app, BrowserWindow} = require("electron");
const {execFile} = require("node:child_process");
const {promisify} = require("node:util");
const path = require("node:path");
const assert = require("node:assert/strict");
const exec = promisify(execFile);
const drive = (mode, query = "") => exec("python3", [path.join(__dirname, "emoji-electron-input.py"),
    "--drive", mode, String(process.pid), query], {timeout: 10000});
app.whenReady().then(async () => {
    const window = new BrowserWindow({width: 700, height: 650, title: "Bingux Electron input test"});
    await window.loadURL("data:text/html," + encodeURIComponent('<body style="padding:80px 40px;font:24px sans-serif">'
        + '<input id="input" style="font:inherit;width:400px"><textarea id="area" style="font:inherit;margin-top:70px"></textarea>'
        + '<div id="edit" contenteditable style="margin-top:70px;min-height:32px"></div>'));
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
            await drive("exercise", query);
            const actual = await window.webContents.executeJavaScript(`(() => {
                const e = document.getElementById(${JSON.stringify(id)});
                return e.isContentEditable ? e.textContent : e.value;
            })()`);
            assert.equal(actual, expected);
            console.log(`PASS Electron ${id}: ${query}, caret placement, Win+Period and Enter`);
        }
        await window.webContents.executeJavaScript("document.activeElement.blur()");
        await drive("blur");
        console.log("PASS Electron input blur clears the caret and rejects insertion");
        app.exit(0);
    } catch (error) {
        console.error(error);
        app.exit(1);
    }
});
