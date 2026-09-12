import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ShellPopup {
    id: contextMenu
    required property var notes
    required property var editor
    property bool headingChoices: false
    function focusMenu() {
        menuNavigation.focusMenu();
    }
    function showHeadings(value) {
        headingChoices = value;
        menuScroll.contentY = 0;
        Qt.callLater(menuNavigation.focusMenu);
    }
    objectName: "notesContextMenu"
    screen: notes.screen
    hostItem: notes.menuHost
    cornerRadius: Theme.radius
    popupWidth: 236
    contentPadding: 6
    surfaceColor: Theme.popupSurface
    popupHeight: Math.min(menuColumn.implicitHeight + 12, height - Theme.barHeight - 24)
    onVisibleChanged: {
        if (visible)
            headingChoices = false;
        else if (notes.visible)
            Qt.callLater(notes.focusContent);
    }
    MenuNavigator {
        id: menuNavigation
        entries: menuColumn.children
        focusTarget: menuColumn
        onEscapeRequested: {
            if (contextMenu.headingChoices)
                contextMenu.showHeadings(false);
            else
                contextMenu.visible = false;
        }
        onActivateRequested: entry => entry.clicked()
        onCurrentEntryChanged: {
            if (!keyboardNavigation || !currentEntry)
                return;
            if (currentEntry.y < menuScroll.contentY)
                menuScroll.contentY = currentEntry.y;
            else if (currentEntry.y + currentEntry.height > menuScroll.contentY + menuScroll.height)
                menuScroll.contentY = currentEntry.y + currentEntry.height - menuScroll.height;
        }
    }
    Flickable {
        id: menuScroll
        anchors.fill: parent
        contentHeight: menuColumn.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        ColumnLayout {
            id: menuColumn
            width: parent.width
            spacing: 0
            Keys.forwardTo: [menuNavigation]
            Repeater {
                model: contextMenu.headingChoices ? [
                    {
                        id: "back",
                        title: "Back",
                        key: "‹"
                    },
                    {
                        id: "heading1",
                        title: "Heading 1",
                        divider: true
                    },
                    {
                        id: "heading2",
                        title: "Heading 2"
                    },
                    {
                        id: "heading3",
                        title: "Heading 3"
                    },
                    {
                        id: "heading4",
                        title: "Heading 4"
                    },
                    {
                        id: "heading5",
                        title: "Heading 5"
                    },
                    {
                        id: "heading6",
                        title: "Heading 6"
                    }
                ] : [
                    {
                        id: "undo",
                        title: "Undo",
                        key: "Ctrl+Z"
                    },
                    {
                        id: "redo",
                        title: "Redo",
                        key: "Ctrl+Shift+Z"
                    },
                    {
                        id: "cut",
                        title: "Cut",
                        key: "Ctrl+X",
                        divider: true
                    },
                    {
                        id: "copy",
                        title: "Copy",
                        key: "Ctrl+C"
                    },
                    {
                        id: "paste",
                        title: "Paste",
                        key: "Ctrl+V"
                    },
                    {
                        id: "selectAll",
                        title: "Select all",
                        key: "Ctrl+A"
                    },
                    {
                        id: "bold",
                        title: "Bold",
                        key: "Ctrl+B",
                        divider: true
                    },
                    {
                        id: "italic",
                        title: "Italic",
                        key: "Ctrl+I"
                    },
                    {
                        id: "strikeout",
                        title: "Strikethrough"
                    },
                    {
                        id: "headings",
                        title: "Heading",
                        key: "›",
                        divider: true
                    },
                    {
                        id: "bullet",
                        title: "Bulleted list"
                    },
                    {
                        id: "numbered",
                        title: "Numbered list"
                    },
                    {
                        id: "quote",
                        title: "Quote"
                    },
                    {
                        id: "code",
                        title: "Code block"
                    },
                    {
                        id: "plain",
                        title: "Plain text"
                    }
                ]
                AbstractButton {
                    id: menuAction
                    required property var modelData
                    readonly property bool menuEntry: true
                    objectName: "notesAction_" + modelData.id
                    Layout.fillWidth: true
                    Layout.topMargin: modelData.divider ? 9 : 0
                    implicitHeight: 28
                    hoverEnabled: true
                    enabled: notes.menuEnabled(modelData.id)
                    Accessible.name: modelData.title
                    Keys.forwardTo: [menuNavigation]
                    onHoveredChanged: if (hovered)
                        menuNavigation.pointerActivate()
                    onClicked: {
                        if (modelData.id === "headings" || modelData.id === "back") {
                            contextMenu.showHeadings(modelData.id === "headings");
                        } else
                            notes.applyAction(modelData.id);
                    }
                    background: Rectangle {
                        radius: contextMenu.contentRadius
                        color: menuAction.hovered || menuAction.visualFocus ? Theme.hover : "transparent"
                    }
                    Rectangle {
                        visible: !!menuAction.modelData.divider
                        x: 6
                        y: -5
                        width: parent.width - 12
                        height: 1
                        color: Theme.barDivider
                    }
                    contentItem: RowLayout {
                        spacing: 8
                        Text {
                            Layout.leftMargin: 8
                            Layout.fillWidth: true
                            text: menuAction.modelData.title
                            color: menuAction.enabled ? Theme.text : Theme.muted
                            opacity: menuAction.enabled ? 1 : 0.6
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            font.bold: menuAction.modelData.id === "bold"
                            font.italic: menuAction.modelData.id === "italic"
                            font.strikeout: menuAction.modelData.id === "strikeout"
                        }
                        Text {
                            Layout.rightMargin: 8
                            text: menuAction.modelData.key || ""
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                        }
                    }
                }
            }
        }
    }
}
