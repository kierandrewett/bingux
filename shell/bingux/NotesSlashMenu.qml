import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import "NotesCommands.js" as Commands

Popup {
    id: menu
    required property var notes
    required property var editor
    objectName: "notesSlashMenu"
    parent: notes.menuHost || notes
    property int tokenStart: -1
    property string query: ""
    property var matches: Commands.search(query)
    property int selectedIndex: 0
    readonly property var selectedCommand: matches[selectedIndex] || null
    readonly property point caret: {
        // Mapping alone does not subscribe to scrolling or reparenting.
        for (let item = editor; item; item = item.parent) {
            const geometry = [item.x, item.y, item.width, item.height];
        }
        const rect = editor.cursorRectangle;
        return editor.mapToItem(parent, rect.x, rect.y);
    }
    width: Math.min(308, parent.width - 16)
    height: Math.min(330, parent.height - 16, 66 + Math.max(1, matches.length) * 48)
    x: Math.max(8, Math.min(caret.x, parent.width - width - 8))
    y: caret.y + editor.cursorRectangle.height + height + 8 <= parent.height
        ? caret.y + editor.cursorRectangle.height + 4 : Math.max(8, caret.y - height - 4)
    padding: 6
    focus: false
    modal: false
    popupType: Popup.Item
    closePolicy: Popup.CloseOnPressOutside
    onClosed: tokenStart = -1
    onQueryChanged: selectedIndex = 0
    onSelectedIndexChanged: list.positionViewAtIndex(selectedIndex, ListView.Contain)
    background: Rectangle { color: Qt.rgba(Theme.surface.r, Theme.surface.g, Theme.surface.b, 1); radius: Theme.radius; border.width: 1; border.color: Theme.outline }

    function update(typed) {
        if (editor.formatting) return;
        const end = editor.cursorPosition;
        const before = editor.getText(0, end);
        const current = Commands.token(before);
        if (!editor.activeFocus || editor.inputMethodComposing || editor.selectionStart !== editor.selectionEnd
            || !current || (tokenStart >= 0 && current.start !== tokenStart)
            || /^\s*```/.test(editor.getFormattedText(editor.activeBlockStart, end))) {
            close();
            return;
        }
        if (tokenStart < 0 && !(typed && before.endsWith("/"))) return;
        tokenStart = current.start;
        query = current.query;
        if (!visible) { selectedIndex = 0; notes.closeContextMenu(); open(); }
    }
    function handleKey(event) {
        if (!visible || editor.inputMethodComposing) return false;
        if (event.key === Qt.Key_Escape) close();
        else if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            const count = matches.length;
            if (count) selectedIndex = (selectedIndex + (event.key === Qt.Key_Down ? 1 : count - 1)) % count;
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Tab) {
            if (selectedCommand) insertCommand(selectedCommand);
        } else return false;
        return true;
    }
    function insertCommand(command) {
        const start = tokenStart, end = editor.cursorPosition;
        if (start < 0 || !editor.getText(start, end).startsWith("/")) { close(); return; }
        close();
        notes.insertCommand(command, start, end);
    }
    contentItem: ColumnLayout {
        spacing: 4
        Text {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.topMargin: 4
            text: menu.query ? "Insert /" + menu.query : "Insert a block or style"
            elide: Text.ElideRight
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: 12
        }
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: menu.matches
            currentIndex: menu.selectedIndex
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Accessible.role: Accessible.List
            Accessible.name: "Note commands"
            delegate: AbstractButton {
                id: entry
                required property var modelData
                required property int index
                objectName: "notesCommand_" + modelData.id
                width: list.width
                height: 48
                focusPolicy: Qt.NoFocus
                hoverEnabled: true
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.title
                Accessible.description: modelData.hint
                Accessible.selected: menu.selectedIndex === index
                onClicked: menu.insertCommand(modelData)
                background: Rectangle {
                    radius: Theme.insetRadius(Theme.radius, 6)
                    color: menu.selectedIndex === entry.index || entry.hovered ? Theme.textSelection : "transparent"
                }
                contentItem: RowLayout {
                    spacing: 10
                    Item {
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 24
                        Accessible.ignored: true
                        SymbolicIcon {
                            anchors.centerIn: parent
                            implicitSize: 20
                            visible: !!entry.modelData.icon
                            source: !entry.modelData.icon ? "" : entry.modelData.icon.startsWith("icons/")
                                ? Qt.resolvedUrl(entry.modelData.icon) : Quickshell.iconPath(entry.modelData.icon)
                            color: Theme.text
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: !!entry.modelData.iconLabel
                            text: entry.modelData.iconLabel || ""
                            font.family: Theme.fontFamily
                            font.pixelSize: 16
                            font.weight: Font.Medium
                            color: Theme.text
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text { Layout.fillWidth: true; text: entry.modelData.title; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 14; elide: Text.ElideRight }
                        Text { Layout.fillWidth: true; text: entry.modelData.hint; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 11; elide: Text.ElideRight }
                    }
                }
            }
            Text {
                anchors.centerIn: parent
                visible: menu.matches.length === 0
                text: "No matching commands"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: 13
            }
        }
        Text {
            Layout.leftMargin: 8
            Layout.bottomMargin: 4
            text: "↑↓ Choose   Enter Insert   Esc Close"
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: 11
        }
    }
}
