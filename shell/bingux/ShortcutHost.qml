import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Approval for the org.gnoblin.Shell.GlobalShortcutsProvider owner in bingux-portal. The provider
// asks for one request, polls its result, and cancels it when the portal caller goes away. One
// request is shown at a time; a second request is refused until the first one ends.
Scope {
    id: root

    readonly property int maxShortcuts: 64
    readonly property int keepFinished: 120000
    // Request records by ID. Finished records stay so the provider can read the decision.
    property var requests: ({})
    property string activeId: ""
    readonly property var active: activeId ? record(activeId) : null
    // One flag per shortcut row in the active request. Rows default to allowed: the user
    // confirms the whole set with Allow and can switch rows off before that.
    property var checkedRows: []
    readonly property bool approvable: active !== null && checkedRows.some((value, index) => value && active.shortcuts[index].accelerator !== "")

    // Own keys only: an ID such as "__proto__" must not reach Object.prototype.
    function record(id) {
        return Object.prototype.hasOwnProperty.call(requests, id) ? requests[id] : null;
    }

    function requestError(id) {
        if (!/^[A-Za-z0-9._-]{1,64}$/.test(id) || id === "__proto__")
            return "Invalid request ID";
        pruneRequests();
        if (record(id))
            return "Request ID already used";
        if (activeId)
            return "Another shortcut request is pending";
        return "";
    }

    function parseShortcuts(raw) {
        let parsed;
        try {
            parsed = JSON.parse(raw);
        } catch (error) {
            return {
                ok: false,
                error: "Request is not JSON"
            };
        }
        if (parsed.mode !== "bind" && parsed.mode !== "configure")
            return {
                ok: false,
                error: "Unknown shortcut mode"
            };
        const request = parsed.request ?? {};
        if (typeof request.name !== "string" || request.name === "")
            return {
                ok: false,
                error: "Request needs a name"
            };
        const list = Array.isArray(request.shortcuts) ? request.shortcuts : [];
        if (list.length === 0 || list.length > maxShortcuts)
            return {
                ok: false,
                error: "Request needs 1 to " + maxShortcuts + " shortcuts"
            };
        const shortcuts = [];
        for (const entry of list) {
            if (typeof entry?.id !== "string" || typeof entry.accelerator !== "string")
                return {
                    ok: false,
                    error: "Each shortcut needs an id and an accelerator"
                };
            shortcuts.push({
                id: entry.id,
                description: typeof entry.description === "string" ? entry.description : "",
                accelerator: entry.accelerator
            });
        }
        return {
            ok: true,
            mode: parsed.mode,
            name: request.name,
            shortcuts: shortcuts
        };
    }

    function beginRequest(id, raw) {
        const error = requestError(id);
        if (error)
            return {
                ok: false,
                error: error
            };
        const parsed = parseShortcuts(raw);
        if (!parsed.ok)
            return parsed;
        setRequest(id, {
            id: id,
            mode: parsed.mode,
            name: parsed.name,
            shortcuts: parsed.shortcuts,
            state: "pending",
            approved: [],
            finished: 0
        });
        activeId = id;
        checkedRows = parsed.shortcuts.map(shortcut => shortcut.accelerator !== "");
        return {
            ok: true
        };
    }

    function setRequest(id, record) {
        const next = Object.assign({}, requests);
        next[id] = record;
        requests = next;
    }

    function finish(state, approved) {
        if (!activeId)
            return;
        const record = Object.assign({}, requests[activeId], {
            state: state,
            approved: approved,
            finished: Date.now()
        });
        setRequest(activeId, record);
        activeId = "";
        checkedRows = [];
        pruneRequests();
    }

    function approve() {
        if (!approvable)
            return;
        const approved = [];
        active.shortcuts.forEach((shortcut, index) => {
            if (checkedRows[index] && shortcut.accelerator !== "")
                approved.push({
                    id: shortcut.id,
                    accelerator: shortcut.accelerator
                });
        });
        finish("approved", approved);
    }

    function deny() {
        finish("denied", []);
    }

    function setChecked(index, value) {
        const next = checkedRows.slice();
        next[index] = value;
        checkedRows = next;
    }

    function requestStatus(id) {
        pruneRequests();
        const entry = record(id);
        if (!entry)
            return {
                id: id,
                state: "unknown"
            };
        return {
            id: id,
            state: entry.state,
            approved: entry.state === "approved" ? entry.approved : []
        };
    }

    function cancelRequest(id) {
        if (!activeId || id !== activeId)
            return {
                ok: false,
                error: "Request is not pending"
            };
        finish("cancelled", []);
        return {
            ok: true
        };
    }

    function pruneRequests() {
        const now = Date.now();
        const next = {};
        let changed = false;
        for (const id of Object.keys(requests)) {
            const entry = requests[id];
            if (entry.finished && now - entry.finished > keepFinished) {
                changed = true;
                continue;
            }
            next[id] = entry;
        }
        if (changed)
            requests = next;
    }

    IpcHandler {
        target: "shortcuts"
        function requestShortcuts(id: string, raw: string): string {
            return JSON.stringify(root.beginRequest(id, raw));
        }
        function requestResult(id: string): string {
            return JSON.stringify(root.requestStatus(id));
        }
        function cancelRequest(id: string): string {
            return JSON.stringify(root.cancelRequest(id));
        }
    }

    PanelWindow {
        id: dialog
        visible: root.active !== null
        color: "transparent"
        implicitWidth: 460
        implicitHeight: card.implicitHeight + Theme.padding * 2
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "bingux-shortcuts"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

        Rectangle {
            id: card
            anchors.centerIn: parent
            width: parent.width - Theme.padding * 2
            implicitHeight: content.implicitHeight + Theme.padding * 2
            height: implicitHeight
            radius: Theme.shellRadius
            color: Theme.shellSurface
            border.width: 1
            border.color: Theme.outline
            focus: dialog.visible
            Keys.onEscapePressed: root.deny()

            ColumnLayout {
                id: content
                anchors.fill: parent
                anchors.margins: Theme.padding
                spacing: Theme.gap

                Text {
                    Layout.fillWidth: true
                    text: root.active ? (root.active.mode === "configure" ? "Change shortcuts for " : "Allow shortcuts for ") + root.active.name : ""
                    textFormat: Text.PlainText
                    color: Theme.text
                    font.pixelSize: Theme.fontSize + 2
                    font.bold: true
                    wrapMode: Text.WordWrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }
                Text {
                    Layout.fillWidth: true
                    text: "Pressing an allowed shortcut sends it to this application, even when another window has focus."
                    color: Theme.muted
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }
                Repeater {
                    model: root.active ? root.active.shortcuts : []
                    delegate: RowLayout {
                        id: row
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        spacing: Theme.gap
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Text {
                                Layout.fillWidth: true
                                text: row.modelData.description || row.modelData.id
                                textFormat: Text.PlainText
                                color: Theme.text
                                font.pixelSize: Theme.fontSize
                                wrapMode: Text.WordWrap
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: row.modelData.accelerator || "No shortcut suggested"
                                textFormat: Text.PlainText
                                color: Theme.muted
                                font.pixelSize: Theme.fontSize - 1
                                elide: Text.ElideRight
                            }
                        }
                        ControlSwitch {
                            enabled: row.modelData.accelerator !== ""
                            checked: root.checkedRows[row.index] ?? false
                            onToggled: root.setChecked(row.index, checked)
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
                        text: "Deny"
                        onClicked: root.deny()
                    }
                    ActionButton {
                        text: "Allow"
                        enabled: root.approvable
                        onClicked: root.approve()
                    }
                }
            }
        }
    }
}
