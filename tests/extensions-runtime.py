#!/usr/bin/env python3
"""Exercise real QML loading, previews, disabled input and extension unload."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="bingux-extension-test-") as temporary:
    folder = Path(temporary)
    shell = folder / "shell"
    shutil.copytree(ROOT / "shell/bingux", shell)
    extension = folder / "data/bingux/extensions/example"
    extension.mkdir(parents=True)
    (extension / "extension.json").write_text(json.dumps({"id": "example", "name": "Example", "apiVersion": 1,
        "entry": "Entry.qml", "widgets": [{"id": "sample", "name": "Sample", "component": "Widget.qml"}]}))
    (extension / "Widget.qml").write_text('''import QtQuick
Rectangle {
    required property var context
    implicitWidth: 90; implicitHeight: 28
    property string value: context.preview ? "Sample" : "Live"
    color: context.theme.accent
}
''')
    (extension / "Entry.qml").write_text('''import QtQuick
Item {
    required property var context
    Component.onCompleted: context.unstable.marker = "started"
    Component.onDestruction: context.unstable.marker = "stopped"
}
''')
    config = folder / "config/bingux"
    config.mkdir(parents=True)
    (config / "extensions.json").write_text('{"enabled":["example"]}')
    (shell / "shell.qml").write_text('''import QtQuick
import Quickshell
ShellRoot {
    id: root
    property string marker: ""
    ExtensionServices { shellObjects: root }
    ExtensionWidget { id: live; widgetId: "extension:example/sample" }
    ExtensionWidget { id: preview; widgetId: live.widgetId; preview: true }
    property int step: 0
    Timer {
        interval: 100; running: true; repeat: true
        onTriggered: {
            try {
                if (root.step === 0 && live.children[0].item && preview.children[0].item && root.marker === "started") {
                    if (live.children[0].item.value !== "Live" || preview.children[0].item.value !== "Sample") throw Error("Preview context");
                    if (preview.enabled || !live.enabled) throw Error("Input state");
                    DesktopEditing.editor = {visible: true, desktop: {}, layout: {}};
                    root.step = 1;
                } else if (root.step === 1) {
                    if (live.enabled) throw Error("Editing input state");
                    ExtensionRegistry.widgets = [];
                    ExtensionRegistry.extensions = [];
                    root.step = 2;
                } else if (root.step === 2) {
                    if (live.children[0].item || root.marker !== "stopped") throw Error("Unload lifecycle");
                    console.log("EXTENSION_TEST_PASS");
                    Qt.quit();
                }
            } catch (error) { console.error("EXTENSION_TEST_FAIL: " + error); Qt.quit(); }
        }
    }
    Timer { interval: 8000; running: true; onTriggered: { console.error("EXTENSION_TEST_TIMEOUT"); Qt.quit(); } }
}
''')
    environment = os.environ | {"XDG_DATA_HOME": str(folder / "data"), "XDG_DATA_DIRS": str(folder / "system"),
        "XDG_CONFIG_HOME": str(folder / "config"), "QT_QPA_PLATFORM": "offscreen", "BINGUX_NO_EXTENSIONS": "0"}
    result = subprocess.run([os.environ.get("QS_TEST_BIN", "qs"), "-p", str(shell), "--no-color"],
        env=environment, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=20)
    print(result.stdout)
    if "EXTENSION_TEST_PASS" not in result.stdout or any(term in result.stdout for term in ["ReferenceError", "TypeError", "Binding loop", "EXTENSION_TEST_FAIL"]):
        raise SystemExit(1)
