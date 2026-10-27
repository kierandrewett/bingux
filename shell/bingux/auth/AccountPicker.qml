import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "Availability.js" as Availability

// The account list that slides up from the bottom edge: each account with its
// picture, name and the real state of its session (Running, locked; Not
// signed in; Starting; Unavailable).
//
// Choosing a row only reports `chosen(uid)`. It never starts or reveals a
// session; the host turns that into an appearance change and a new sign-in
// prompt. Keyboard: Up, Down, Home, End move; Return or Space chooses; Escape
// closes. Touch and mouse: tap a row, tap outside or drag the handle down to
// close, flick a long list. Rows are exposed to assistive technology with
// their state in the name.
Item {
    id: root

    property AuthTheme theme: AuthTheme {}
    property Item backdrop: null
    // [{uid, userName, displayName, avatar, availability}]
    property var users: []
    property int selectedUid: -1
    property bool open: false
    readonly property alias list: list

    signal chosen(int uid)
    signal dismissed

    property real reveal: open ? 1 : 0
    Behavior on reveal {
        NumberAnimation {
            duration: root.theme.enterMotion
            easing.type: root.open ? Easing.OutCubic : Easing.InCubic
        }
    }
    visible: reveal > 0

    function indexOfUid(uid) {
        for (let i = 0; i < users.length; i++) {
            if (users[i].uid === uid)
                return i;
        }
        return -1;
    }

    function choose(index) {
        if (index < 0 || index >= users.length)
            return;
        if (!Availability.selectable(users[index].availability))
            return;
        chosen(users[index].uid);
    }

    onOpenChanged: {
        if (open) {
            list.currentIndex = Math.max(0, indexOfUid(selectedUid));
            list.forceActiveFocus();
        }
    }

    // Tap outside the sheet to close it.
    MouseArea {
        anchors.fill: parent
        enabled: root.open
        onClicked: root.dismissed()
        Rectangle {
            anchors.fill: parent
            color: Qt.rgba(0, 0, 0, 0.32)
            opacity: root.reveal
        }
    }

    Item {
        id: sheet
        readonly property int rowHeight: 60
        readonly property int margin: Math.max(Theme.paddingLarge, Math.round(root.height * 0.04))
        width: Math.min(root.width - Theme.paddingLarge * 2, 440)
        height: Math.min(handle.height + list.contentHeight + Theme.gap * 2, Math.round(root.height * 0.62))
        x: Math.round((root.width - width) / 2)
        y: root.height - (height + margin) * root.reveal
        opacity: Math.min(1, root.reveal * 1.6)

        SessionGlass {
            anchors.fill: parent
            backdrop: root.backdrop
            radius: Theme.cardRadius
            tint: root.theme.darkInk ? Qt.rgba(1, 1, 1, 0.7) : Theme.softwareRendering ? Qt.rgba(0.12, 0.12, 0.14, 0.96) : Qt.rgba(0.1, 0.1, 0.12, 0.42)
            outline: root.theme.glassOutline
        }

        Item {
            id: handle
            width: parent.width
            height: 24
            Rectangle {
                anchors.centerIn: parent
                width: 36
                height: 4
                radius: 2
                color: root.theme.inkMuted
                opacity: 0.6
            }
            DragHandler {
                // translation is back at zero by the time `active` turns false,
                // so the travel is kept while the drag runs.
                property real travelled: 0
                target: null
                xAxis.enabled: false
                onTranslationChanged: if (active)
                    travelled = translation.y
                onActiveChanged: {
                    if (active)
                        travelled = 0;
                    else if (travelled > 64)
                        root.dismissed();
                }
            }
        }

        ListView {
            id: list
            objectName: "accountList"
            anchors.top: handle.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: Theme.gap
            anchors.topMargin: 0
            clip: true
            model: root.users
            currentIndex: 0
            keyNavigationEnabled: false
            boundsBehavior: Flickable.StopAtBounds
            activeFocusOnTab: true
            Accessible.role: Accessible.List
            Accessible.name: "Accounts"

            Keys.onUpPressed: currentIndex = Math.max(0, currentIndex - 1)
            Keys.onDownPressed: currentIndex = Math.min(count - 1, currentIndex + 1)
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Home) {
                    currentIndex = 0;
                    event.accepted = true;
                } else if (event.key === Qt.Key_End) {
                    currentIndex = count - 1;
                    event.accepted = true;
                } else if (event.key === Qt.Key_Escape) {
                    root.dismissed();
                    event.accepted = true;
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
                    root.choose(currentIndex);
                    event.accepted = true;
                }
            }
            onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)

            delegate: AbstractButton {
                id: row
                required property var modelData
                required property int index
                readonly property bool usable: Availability.selectable(modelData.availability)
                readonly property bool current: ListView.isCurrentItem && list.activeFocus
                width: ListView.view.width
                height: sheet.rowHeight
                hoverEnabled: true
                enabled: usable
                Accessible.role: Accessible.ListItem
                Accessible.name: modelData.displayName + ", " + Availability.label(modelData.availability)
                Accessible.selected: modelData.uid === root.selectedUid
                onClicked: {
                    list.currentIndex = index;
                    root.choose(index);
                }
                background: Rectangle {
                    radius: Theme.radius
                    color: row.down ? Qt.rgba(1, 1, 1, 0.26) : row.hovered || row.current ? Qt.rgba(1, 1, 1, 0.16) : row.modelData.uid === root.selectedUid ? Qt.rgba(1, 1, 1, 0.1) : "transparent"
                    border.width: row.current ? 1 : 0
                    border.color: root.theme.glassOutlineFocus
                }
                contentItem: Item {
                    opacity: row.usable ? 1 : 0.5
                    UserAvatar {
                        id: avatar
                        anchors.left: parent.left
                        anchors.leftMargin: Theme.padding
                        anchors.verticalCenter: parent.verticalCenter
                        size: 40
                        source: row.modelData.avatar
                        name: row.modelData.displayName
                    }
                    Column {
                        anchors.left: avatar.right
                        anchors.leftMargin: Theme.padding
                        anchors.right: tick.left
                        anchors.rightMargin: Theme.gap
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1
                        Text {
                            textFormat: Text.PlainText
                            width: parent.width
                            text: row.modelData.displayName
                            color: root.theme.ink
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                            font.weight: Font.DemiBold
                            elide: Text.ElideRight
                        }
                        Row {
                            spacing: 6
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 7
                                height: 7
                                radius: 3.5
                                visible: row.modelData.availability === "running_locked" || row.modelData.availability === "running_foreground"
                                color: Theme.success
                            }
                            Text {
                                textFormat: Text.PlainText
                                text: Availability.label(row.modelData.availability)
                                color: root.theme.inkMuted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                        }
                    }
                    SymbolicIcon {
                        id: tick
                        anchors.right: parent.right
                        anchors.rightMargin: Theme.padding
                        anchors.verticalCenter: parent.verticalCenter
                        visible: row.modelData.uid === root.selectedUid
                        implicitSize: 14
                        source: AdwaitaIcons.source("object-select-symbolic")
                        color: root.theme.ink
                    }
                }
            }
        }
    }
}
