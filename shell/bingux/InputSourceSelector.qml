import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

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

        return "KB " + metrics.inputSourceLabel.toUpperCase();
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
        color: (selectorMouse.containsMouse || root.activeFocus) && root.canSelect ? "#2b3545" : "transparent"
    }

    Text {
        id: inputLabel

        anchors.centerIn: parent
        color: root.metrics.desktopStateAvailable ? "#d9dee8" : "#8b94a3"
        font.pixelSize: 12
        text: root.displayLabel
    }

    MouseArea {
        id: selectorMouse

        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        cursorShape: root.canSelect ? Qt.PointingHandCursor : Qt.ArrowCursor
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

    PopupWindow {
        id: inputMenu

        visible: root.menuOpen
        implicitWidth: 220
        implicitHeight: menuSurface.implicitHeight
        color: "transparent"
        grabFocus: true
        onVisibleChanged: {
            if (!visible)
                root.menuOpen = false;

        }

        anchor {
            item: root
            edges: Edges.Bottom | Edges.Left
            gravity: Edges.Bottom | Edges.Right
            adjustment: PopupAdjustment.Flip | PopupAdjustment.Slide
            margins.top: 6
        }

        Rectangle {
            id: menuSurface

            width: inputMenu.width
            height: implicitHeight
            implicitHeight: menuColumn.implicitHeight + 12
            radius: 8
            color: "#202632"
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

            Column {
                id: menuColumn

                spacing: 2

                anchors {
                    fill: parent
                    margins: 6
                }

                Text {
                    width: parent.width
                    color: "#aeb8ca"
                    font.pixelSize: 12
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

                        width: parent.width
                        height: 34
                        Accessible.name: sourceAction.modelData.displayName
                        Accessible.role: Accessible.Button

                        Rectangle {
                            anchors.fill: parent
                            radius: 6
                            color: sourceActionMouse.containsMouse || root.selectedIndex === index ? "#344158" : "transparent"
                        }

                        Text {
                            color: "#edf1f7"
                            elide: Text.ElideRight
                            font.pixelSize: 13
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
                            cursorShape: root.canSelect ? Qt.PointingHandCursor : Qt.ArrowCursor
                            enabled: root.canSelect
                            onClicked: root.selectSource(sourceAction.modelData)
                        }

                    }

                }

                Text {
                    width: parent.width
                    visible: root.lastError !== ""
                    color: "#f4a340"
                    font.pixelSize: 12
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
