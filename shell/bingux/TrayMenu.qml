import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQml.Models
import Quickshell

ShellPopup {
    id: root
    property var menu: null
    property var parents: []
    property var currentMenu: menu
    readonly property bool keyboardNavigation: navigation.keyboardNavigation
    readonly property real monitorWidthLimit: screen ? Math.floor(screen.width * 0.30) : 576
    readonly property real measuredWidth: {
        let widest = 0;
        for (let index = 0; index < menuTextMetrics.count; index++) {
            const metric = menuTextMetrics.objectAt(index);
            if (metric)
                widest = Math.max(widest, metric.width);
        }
        const contentWidth = widest + 18 + Theme.gap + 14 + contentPadding * 2 + Theme.gap;
        return Math.min(monitorWidthLimit, Math.max(220, Math.ceil(contentWidth)));
    }
    popupWidth: measuredWidth
    contentPadding: Theme.gap
    popupHeight: Math.min(620, entries.contentHeight + contentPadding * 2 + (parents.length ? 40 : 0))
    onMenuChanged: { parents = []; currentMenu = menu }
    onVisibleChanged: if (visible) navigation.focusMenu()
    // Populate menu entries while the tray icon is hovered, before a click
    // needs to show the menu surface.
    QsMenuOpener { id: opener; menu: root.currentMenu }
    MenuNavigator {
        id: navigation
        entries: opener.children.values
        view: entries
        focusTarget: entries
        onEscapeRequested: root.visible = false
        onActivateRequested: root.activate(entry)
    }
    Instantiator {
        id: menuTextMetrics
        model: opener.children
        delegate: TextMetrics {
            required property var modelData
            text: modelData && typeof modelData.text === "string" ? modelData.text.replace(/&(.)/g, "$1") : ""
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
    }
    function back() {
        if (!parents.length) { visible = false; return }
        const path = parents.slice(); currentMenu = path.pop(); parents = path;
        navigation.focusMenu();
    }
    function activate(entry) {
        if (!entry || !entry.enabled || entry.isSeparator) return;
        if (entry.hasChildren) {
            parents = parents.concat([currentMenu]); currentMenu = entry;
            navigation.focusMenu();
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
            Keys.onDownPressed: function(event) { navigation.move(1); event.accepted = true }
            Keys.onUpPressed: function(event) { navigation.move(-1); event.accepted = true }
            Keys.onReturnPressed: function(event) { navigation.activateCurrent(); event.accepted = true }
            Keys.onSpacePressed: function(event) { navigation.activateCurrent(); event.accepted = true }
            Keys.onLeftPressed: root.back()
            Keys.onRightPressed: if (currentItem && currentItem.modelData.hasChildren) navigation.activateCurrent()
            delegate: ItemDelegate {
                id: entryButton
                required property var modelData
                required property int index
                width: entries.width
                height: modelData.isSeparator ? 9 : 38
                enabled: modelData.enabled && !modelData.isSeparator
                text: modelData.text.replace(/&(.)/g, "$1")
                Accessible.name: text
                onClicked: {
                    navigation.pointerActivate();
                    root.activate(modelData);
                }
                background: Rectangle {
                    radius: root.contentRadius
                    color: entryButton.down ? Theme.pressed : entryButton.enabled && (entryButton.hovered || (root.keyboardNavigation && entries.activeFocus && entries.currentIndex === entryButton.index)) ? Theme.hover : "transparent"
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
