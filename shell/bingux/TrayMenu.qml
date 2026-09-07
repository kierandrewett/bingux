import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell

ShellPopup {
    id: root
    property bool keyboardNavigation: false
    property var menu: null
    property var parents: []
    property var currentMenu: menu
    popupWidth: 300
    contentPadding: Theme.gap
    popupHeight: Math.min(620, entries.contentHeight + contentPadding * 2 + (parents.length ? 40 : 0))
    onMenuChanged: { parents = []; currentMenu = menu }
    onVisibleChanged: if (visible) {
        keyboardNavigation = false;
        entries.currentIndex = -1;
        entries.forceActiveFocus();
    }
    function moveSelection(delta) {
        keyboardNavigation = true;
        const count = entries.count;
        let index = entries.currentIndex;
        if (index < 0) index = delta > 0 ? -1 : 0;
        for (let step = 0; step < count; step++) {
            index = (index + delta + count) % count;
            const entry = opener.children.values[index];
            if (entry && entry.enabled && !entry.isSeparator) {
                entries.currentIndex = index;
                entries.positionViewAtIndex(index, ListView.Contain);
                return;
            }
        }
    }
    QsMenuOpener { id: opener; menu: root.visible ? root.currentMenu : null }
    function back() {
        if (!parents.length) { visible = false; return }
        const path = parents.slice(); currentMenu = path.pop(); parents = path;
    }
    function activate(entry) {
        if (!entry || !entry.enabled || entry.isSeparator) return;
        if (entry.hasChildren) {
            parents = parents.concat([currentMenu]); currentMenu = entry;
            entries.currentIndex = -1;
            keyboardNavigation = false;
        } else { entry.triggered(); visible = false }
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.spaceSmall
        ActionButton {
            cornerRadius: root.contentRadius
            visible: root.parents.length > 0
            text: "Back"
            Layout.fillWidth: true
            onClicked: root.back()
        }
        ListView {
            id: entries
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: opener.children
            clip: true
            spacing: 2
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            Keys.onDownPressed: root.moveSelection(1)
            Keys.onUpPressed: root.moveSelection(-1)
            Keys.onReturnPressed: root.activate(currentItem ? currentItem.modelData : null)
            Keys.onSpacePressed: root.activate(currentItem ? currentItem.modelData : null)
            Keys.onLeftPressed: root.back()
            Keys.onRightPressed: if (currentItem && currentItem.modelData.hasChildren) root.activate(currentItem.modelData)
            delegate: ItemDelegate {
                id: entryButton
                required property var modelData
                required property int index
                width: entries.width
                height: modelData.isSeparator ? 9 : 38
                enabled: modelData.enabled && !modelData.isSeparator
                text: modelData.text.replace(/&(.)/g, "$1")
                Accessible.name: text
                onClicked: root.activate(modelData)
                background: Rectangle {
                    radius: root.contentRadius
                    color: entryButton.enabled && (entryButton.hovered || (root.keyboardNavigation && entries.activeFocus && entries.currentIndex === entryButton.index)) ? Theme.hover : "transparent"
                    Rectangle { visible: entryButton.modelData.isSeparator; anchors.centerIn: parent; width: parent.width - 12; height: 1; color: Theme.outline }
                }
                contentItem: RowLayout {
                    visible: !entryButton.modelData.isSeparator
                    spacing: Theme.gap
                    Text { Layout.preferredWidth: 18; text: entryButton.modelData.checkState === Qt.Checked ? "✓" : ""; color: Theme.accent }
                    Text { Layout.fillWidth: true; text: entryButton.text; textFormat: Text.PlainText; elide: Text.ElideRight; color: entryButton.enabled ? Theme.text : Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
                    Text { text: entryButton.modelData.hasChildren ? "›" : ""; color: Theme.muted }
                }
            }
        }
    }
}
