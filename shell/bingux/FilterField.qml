import QtQuick
import QtQuick.Controls
import Quickshell

TextField {
    id: root
    implicitHeight: 36
    leftPadding: 32
    rightPadding: 30
    placeholderText: "Search"
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSmall
    color: Theme.text
    placeholderTextColor: Theme.muted
    selectByMouse: true
    selectionColor: Theme.textSelection
    selectedTextColor: Theme.text
    function clearFilter() {
        clear();
        textEdited();
        forceActiveFocus();
    }
    Keys.onEscapePressed: clearFilter()
    SymbolicIcon {
        x: 10
        anchors.verticalCenter: parent.verticalCenter
        implicitSize: 14
        source: Quickshell.iconPath("system-search-symbolic")
        color: Theme.muted
    }
    IconButton {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: 28
        implicitHeight: 28
        visible: root.text.length > 0
        iconName: "edit-clear-symbolic"
        label: "Clear search"
        onClicked: root.clearFilter()
    }
    background: Rectangle {
        radius: 7
        color: Theme.surface
        border.width: root.activeFocus ? 1 : 0
        border.color: Theme.accent
    }
}
