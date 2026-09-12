// Run with an Electron binary, not node. No user profile or network content.
const { app, BrowserWindow } = require("electron");
const { execFile } = require("node:child_process");
const path = require("node:path");
const os = require("node:os");
const fs = require("node:fs");
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "bingux-electron-caret-"));
app.setPath("userData", profile);
app.whenReady().then(async () => {
    const window = new BrowserWindow({ width: 640, height: 520, title: "Bingux Electron caret test" });
    await window.loadURL(
        "data:text/html," +
            encodeURIComponent(
                '<body style="padding:120px 40px"><input autofocus value="Electron caret test" style="font:24px sans-serif;width:400px"><script>const e=document.querySelector("input");e.focus();e.setSelectionRange(e.value.length,e.value.length)</script>',
            ),
    );
    window.focus();
    setTimeout(() => {
        execFile(
            "python3",
            [path.join(__dirname, "../shell/bingux/caret-anchor.py"), String(process.pid)],
            { timeout: 1000 },
            (error, stdout) => {
                const result = stdout.trim();
                console.log(JSON.stringify({ pid: process.pid, error: error?.message, caret: result }));
                app.exit(!error && result && result !== "null" ? 0 : 1);
            },
        );
    }, 1000);
});
