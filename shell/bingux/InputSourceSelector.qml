import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var parentWindow
    required property var metrics
    required property string gnoblinCtlPath
    property bool menuOpen: false
    property int selectedIndex: -1
    property string selectedSourceKey: ""
    property bool selectionExplicit: false
    property string lastError: ""
    readonly property var sources: metrics.desktopStateAvailable ? metrics.inputSources : []
    readonly property bool canSelect: metrics.desktopStateAvailable && sources.length > 0 && !inputProcess.running
    readonly property string displayLabel: {
        if (!metrics.desktopStateAvailable || metrics.inputSourceLabel === "")
            return "KB --";

        return "KB " + metrics.inputSourceLabel.toUpperCase().slice(0, 32);
    }

    function currentSourceIndex() {
        const current = metrics.currentInputSource;
        if (current === null)
            return 0;

        for (let index = 0; index < sources.length; index += 1) {
            const source = sources[index];
            if (source.type === current.type && source.id === current.id)
                return index;

        }
        return 0;
    }

    function sourceKey(source) {
        return source.type + "\n" + source.id;
    }

    function setSelectedIndex(index, explicit) {
        selectedIndex = index;
        selectedSourceKey = index >= 0 && index < sources.length ? sourceKey(sources[index]) : "";
        selectionExplicit = explicit === true;
    }

    function reconcileSelection() {
        if (!menuOpen)
            return ;

        if (sources.length === 0) {
            setSelectedIndex(-1, false);
            menuOpen = false;
            return ;
        }
        if (!selectionExplicit) {
            setSelectedIndex(currentSourceIndex(), false);
            return ;
        }
        for (let index = 0; index < sources.length; index += 1) {
            if (sourceKey(sources[index]) === selectedSourceKey) {
                selectedIndex = index;
                return ;
            }
        }
        setSelectedIndex(currentSourceIndex(), false);
    }

    function openMenu() {
        if (!canSelect)
            return ;

        setSelectedIndex(currentSourceIndex(), false);
        lastError = "";
        menuOpen = true;
    }

    function selectSource(source) {
        if (inputProcess.running)
            return ;

        lastError = "";
        inputProcess.exec([root.gnoblinCtlPath, "set-input-source", source.type, source.id]);
    }

    function selectCurrentSource() {
        if (selectedIndex < 0 || selectedIndex >= sources.length)
            return ;

        selectSource(sources[selectedIndex]);
    }

    implicitWidth: inputLabel.implicitWidth + 12
    implicitHeight: 24
    width: implicitWidth
    height: implicitHeight
    activeFocusOnTab: true
    Accessible.name: metrics.desktopStateAvailable ? "Keyboard layout " + (metrics.currentInputSource === null ? "unavailable" : metrics.currentInputSource.displayName) : "Keyboard layout unavailable"
    Accessible.role: Accessible.Button
    onSourcesChanged: reconcileSelection()
    Keys.onPressed: function(event) {
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Space) {
            root.openMenu();
            event.accepted = true;
        }
    }

    Connections {
        function onCurrentInputSourceChanged() {
            root.reconcileSelection();
        }

        target: metrics
    }

    Rectangle {
        anchors.fill: parent
        radius: 4
        color: (selectorMouse.containsMouse || root.activeFocus) && root.canSelect ? Theme.hover : "transparent"
    }

    Text {
        id: inputLabel

        anchors.centerIn: parent
        color: root.metrics.desktopStateAvailable ? Theme.text : Theme.muted
        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
        textFormat: Text.PlainText
        text: root.displayLabel
    }

    MouseArea {
        id: selectorMouse

        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.ArrowCursor
        onClicked: {
            root.forceActiveFocus();
            root.openMenu();
        }
    }

    Process {
        id: inputProcess

        onExited: function(exitCode) {
            if (exitCode === 0)
                root.menuOpen = false;
            else
                root.lastError = "Could not change keyboard layout";
        }
    }

    ShellPopup {
        id: inputMenu
        visible: root.menuOpen
        screen: root.parentWindow.screen
        popupWidth: 260
        popupHeight: menuSurface.implicitHeight + Theme.padding * 2
        onVisibleChanged: {
            if (!visible) root.menuOpen = false;
            else {
                preferredX = root.mapToItem(root.parentWindow.contentItem, 0, 0).x - popupWidth + root.width;
                menuSurface.forceActiveFocus();
            }
        }

        Rectangle {
            id: menuSurface

            width: parent.width
            height: implicitHeight
            implicitHeight: menuColumn.implicitHeight
            radius: 8
            color: "transparent"
            focus: true
            Keys.onPressed: function(event) {
                if (event.key === Qt.Key_Escape) {
                    root.menuOpen = false;
                    event.accepted = true;
                } else if (event.key === Qt.Key_Up && root.sources.length > 0) {
                    root.setSelectedIndex((root.selectedIndex - 1 + root.sources.length) % root.sources.length, true);
                    event.accepted = true;
                } else if (event.key === Qt.Key_Down && root.sources.length > 0) {
                    root.setSelectedIndex((root.selectedIndex + 1) % root.sources.length, true);
                    event.accepted = true;
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Space) {
                    root.selectCurrentSource();
                    event.accepted = true;
                }
            }

            ColumnLayout {
                id: menuColumn

                spacing: 2

                anchors {
                    left: parent.left
                    right: parent.right
                }

                Text {
                    Layout.fillWidth: true
                    color: Theme.muted
                    font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                    text: "Keyboard layout"
                    leftPadding: 8
                    rightPadding: 8
                    topPadding: 4
                    bottomPadding: 4
                }

                Repeater {
                    model: root.sources

                    delegate: Item {
                        id: sourceAction

                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Layout.preferredHeight: 38
                        Accessible.name: sourceAction.modelData.displayName
                        Accessible.role: Accessible.Button

                        Rectangle {
                            anchors.fill: parent
                            radius: 6
                            color: sourceActionMouse.containsMouse || root.selectedIndex === index ? Theme.selection : "transparent"
                        }

                        Text {
                            color: Theme.text
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                            textFormat: Text.PlainText
                            text: sourceAction.modelData.displayName

                            anchors {
                                left: parent.left
                                right: parent.right
                                leftMargin: 10
                                rightMargin: 10
                                verticalCenter: parent.verticalCenter
                            }

                        }

                        MouseArea {
                            id: sourceActionMouse

                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.ArrowCursor
                            enabled: root.canSelect
                            onClicked: root.selectSource(sourceAction.modelData)
                        }

                    }

                }

                Text {
                    Layout.fillWidth: true
                    visible: root.lastError !== ""
                    color: "#f4a340"
                    font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
                    wrapMode: Text.Wrap
                    text: root.lastError
                    leftPadding: 8
                    rightPadding: 8
                    topPadding: 4
                    bottomPadding: 4
                }

            }

        }

    }

}
