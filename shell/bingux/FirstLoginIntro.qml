import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// First-login introduction: the shortcuts and gestures this desktop configures, and a way to open
// Bingux Settings. Completion is stored in Bingux-owned state at statePath("first-login")/intro.json.
// A user who already has Bingux configuration is marked complete without seeing the card. The card
// is an overlay that does not hold the keyboard or delay startup; it only takes focus while open.
Scope {
    id: root

    readonly property string configDirectory: (Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config") + "/bingux"
    readonly property string stateDirectory: Quickshell.statePath("first-login")
    // Files only user action writes. Notification history and window order are written during normal
    // startup, so they cannot tell an existing user from a new one and are not checked.
    readonly property var userFiles: [configDirectory + "/bingux.toml", configDirectory + "/switcher.json", configDirectory + "/extensions.json", Quickshell.statePath("control-centre") + "/controls.json", Quickshell.statePath("emoji") + "/recent.json"]
    readonly property var entries: [
        {
            keys: "Super",
            text: "Tap on its own to open search for apps, files and settings."
        },
        {
            keys: "Super + .",
            text: "Pick an emoji for the focused text field."
        },
        {
            keys: "Alt + S",
            text: "Capture a screenshot or screen recording."
        },
        {
            keys: "Alt + Tab",
            text: "Switch windows. Add Shift to go backwards; Super + Tab also works."
        },
        {
            keys: "Super + Space",
            text: "Switch keyboard input source. Add Shift to go backwards."
        },
        {
            keys: "Super + Z",
            text: "Snap the focused window into a layout."
        },
        {
            keys: "Right-click wallpaper",
            text: "Change the background or display settings."
        },
        {
            keys: "Clock",
            text: "Open the calendar and upcoming events from the top bar."
        },
        {
            keys: "Indicators",
            text: "Open Control Centre from the top bar, with Lock and Power options."
        }
    ]

    property bool open: false
    property bool stateReady: false

    function show() {
        root.open = true;
    }

    // Dismissing records completion. Closing without dismissing leaves the card to show next login.
    function dismiss() {
        if (!root.open)
            return;
        root.open = false;
        root.writeState(true);
    }

    function writeState(completed) {
        if (stateFile.path === "")
            return;
        stateFile.setText(JSON.stringify({
            version: 1,
            completed
        }));
    }

    function restoreState(text) {
        let completed = false;
        try {
            completed = JSON.parse(text).completed === true;
        } catch (error) {
            console.warn("bingux-introduction: ignoring unreadable state: " + error);
        }
        root.stateReady = true;
        if (!completed)
            root.open = true;
    }

    function finishFirstRun(existing) {
        root.stateReady = true;
        root.open = !existing;
        root.writeState(existing);
    }

    Process {
        id: stateDirectorySetup
        command: ["mkdir", "-p", "-m", "700", root.stateDirectory]
        running: true
        onExited: code => {
            if (code === 0)
                stateFile.path = root.stateDirectory + "/intro.json";
            else
                console.warn("bingux-introduction: could not create the state directory");
        }
    }

    FileView {
        id: stateFile
        printErrors: false
        atomicWrites: true
        onLoaded: root.restoreState(text())
        onLoadFailed: existingUserCheck.running = true
        onSaveFailed: console.warn("bingux-introduction: could not save state")
    }

    // First run with no state file: any existing user file means this is an existing Bingux
    // installation, so the card is recorded as complete without showing.
    Process {
        id: existingUserCheck
        command: ["find"].concat(root.userFiles, ["-maxdepth", "0", "-type", "f", "-print"])
        stdout: StdioCollector {
            id: existingUserFound
        }
        onExited: root.finishFirstRun(existingUserFound.text.trim() !== "")
    }

    IpcHandler {
        target: "introduction"
        function show(): void {
            root.show();
        }
        function dismiss(): void {
            root.dismiss();
        }
        function status(): string {
            return JSON.stringify({
                open: root.open,
                ready: root.stateReady
            });
        }
    }

    PanelWindow {
        id: dialog
        visible: root.open
        color: "transparent"
        implicitWidth: 480
        implicitHeight: content.implicitHeight + Theme.padding * 4
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-introduction"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

        onVisibleChanged: {
            if (visible)
                Qt.callLater(() => gotIt.forceActiveFocus());
        }

        FocusScope {
            id: card
            anchors.fill: parent
            focus: true
            opacity: root.open ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.popupOpenMotion
                }
            }

            Keys.onEscapePressed: root.dismiss()

            Rectangle {
                anchors.fill: parent
                anchors.margins: Theme.padding
                radius: Theme.shellRadius
                color: Theme.shellSurface
                border.width: 1
                border.color: Theme.outline
            }

            ColumnLayout {
                id: content
                anchors.fill: parent
                anchors.margins: Theme.padding * 2
                spacing: Theme.gap

                Text {
                    Layout.fillWidth: true
                    text: "Welcome to Bingux"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize + 4
                    font.bold: true
                }
                Text {
                    Layout.fillWidth: true
                    text: "These shortcuts are set up for this desktop. You can open this again from Bingux Settings."
                    color: Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }
                Repeater {
                    model: root.entries
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Theme.gap * 2
                        Text {
                            Layout.preferredWidth: 160
                            Layout.alignment: Qt.AlignTop
                            text: modelData.keys
                            textFormat: Text.PlainText
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                            font.bold: true
                            wrapMode: Text.WordWrap
                        }
                        Text {
                            Layout.fillWidth: true
                            text: modelData.text
                            textFormat: Text.PlainText
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                            wrapMode: Text.WordWrap
                        }
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: Theme.gap
                    spacing: Theme.gap
                    Item {
                        Layout.fillWidth: true
                    }
                    ActionButton {
                        text: "Open Settings"
                        onClicked: {
                            Quickshell.execDetached([Python.python, Python.applicationSpawn, "--", Python.settings]);
                            root.dismiss();
                        }
                    }
                    ActionButton {
                        id: gotIt
                        text: "Got it"
                        onClicked: root.dismiss()
                    }
                }
            }
        }
    }
}
