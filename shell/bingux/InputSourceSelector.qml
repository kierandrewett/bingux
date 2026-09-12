import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var parentWindow
    required property var metrics
    required property string gnoblinCtlPath
    property alias menuOpen: inputMenu.visible
    property bool shortcutsEnabled: true
    property var presentation: null
    signal opening
    property bool cycling: false
    property var pendingSource: null
    property bool keepOpenAfterSelection: false
    property int selectedIndex: -1
    property string selectedSourceKey: ""
    property bool selectionExplicit: false
    property string lastError: ""
    readonly property var sources: metrics.desktopStateAvailable ? metrics.inputSources : []
    readonly property bool selectionBusy: inputProcess.running || pendingSource !== null
    readonly property bool canSelect: metrics.desktopStateAvailable && sources.length > 0 && !inputProcess.running
    readonly property string displayLabel: {
        if (!metrics.desktopStateAvailable || metrics.inputSourceLabel === "")
            return "--";

        return metrics.inputSourceLabel.slice(0, 12);
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
            return;

        if (sources.length === 0) {
            setSelectedIndex(-1, false);
            menuOpen = false;
            return;
        }
        if (!selectionExplicit) {
            setSelectedIndex(currentSourceIndex(), false);
            return;
        }
        for (let index = 0; index < sources.length; index += 1) {
            if (sourceKey(sources[index]) === selectedSourceKey) {
                selectedIndex = index;
                return;
            }
        }
        setSelectedIndex(currentSourceIndex(), false);
    }

    function openMenu() {
        if (!canSelect)
            return;

        cycling = false;
        setSelectedIndex(currentSourceIndex(), false);
        lastError = "";
        menuOpen = true;
    }

    function selectSource(source, keepOpen) {
        lastError = "";
        pendingSource = source;
        keepOpenAfterSelection = keepOpen === true;
        dispatchSelection();
    }

    function dispatchSelection() {
        if (inputProcess.running || !pendingSource)
            return;
        const source = pendingSource;
        pendingSource = null;
        if (!sources.some(item => sourceKey(item) === sourceKey(source)))
            return;
        inputProcess.exec([root.gnoblinCtlPath, "input", "select", source.type, source.id]);
    }

    function cycleSource(backward) {
        if (sources.length === 0)
            return;
        const index = menuOpen && selectedIndex >= 0 ? selectedIndex : currentSourceIndex();
        cycling = true;
        setSelectedIndex((index + (backward ? sources.length - 1 : 1)) % sources.length, true);
        menuOpen = true;
        inputNavigation.currentIndex = selectedIndex;
        selectSource(sources[selectedIndex], true);
    }

    function finishCycle() {
        if (cycling)
            menuOpen = false;
    }

    ShortcutSession {
        enabled: root.shortcutsEnabled
        bindings: [
            {
                id: "keyboard-forward",
                accelerator: "<Super>space",
                hold: 67108864,
                modal: false
            },
            {
                id: "keyboard-backward",
                accelerator: "<Super><Shift>space",
                hold: 67108864,
                modal: false
            }
        ]
        onActivated: function (id) {
            root.cycleSource(id === "keyboard-backward");
        }
        onReleased: root.finishCycle()
        onCancelled: root.finishCycle()
        onFailed: message => console.warn("keyboard switcher: " + message)
    }

    function selectCurrentSource() {
        if (selectedIndex < 0 || selectedIndex >= sources.length)
            return;

        selectSource(sources[selectedIndex]);
    }

    implicitWidth: Math.max(Theme.barIconTarget, (presentation?.custom ? customFace.implicitWidth : labelWidth) + Theme.barPrimaryPadding * 2)
    readonly property real labelWidth: sources.reduce((width, source) => Math.max(width, Math.ceil(labelMetrics.boundingRect(source.shortName || source.id).width)), Math.ceil(labelMetrics.boundingRect("--").width))
    FontMetrics {
        id: labelMetrics
        font: inputLabel.font
    }
    implicitHeight: Theme.barHeight
    width: implicitWidth
    height: implicitHeight
    activeFocusOnTab: true
    Accessible.name: metrics.desktopStateAvailable ? "Keyboard layout " + (metrics.currentInputSource === null ? "unavailable" : metrics.currentInputSource.displayName) : "Keyboard layout unavailable"
    Accessible.role: Accessible.Button
    onSourcesChanged: reconcileSelection()
    Keys.onPressed: function (event) {
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

    BarControlSurface {
        hovered: selectorMouse.containsMouse && root.canSelect
        pressed: selectorMouse.pressed && root.canSelect
        selected: root.menuOpen
        focused: root.activeFocus && root.canSelect
    }

    AnimatedCount {
        id: inputLabel
        visible: !root.presentation?.custom
        objectName: "keyboardLayoutLabel"
        anchors.centerIn: parent
        width: root.labelWidth
        value: root.currentSourceIndex()
        stepValues: false
        formatter: index => root.sources[index] ? (root.sources[index].shortName || root.sources[index].id) : "--"
        color: root.metrics.desktopStateAvailable ? Theme.text : Theme.muted
        font.pixelSize: Theme.fontSize
        font.weight: Font.DemiBold
    }
    WidgetFace {
        id: customFace
        anchors.centerIn: parent
        visible: !!root.presentation?.custom
        presentation: root.presentation
    }

    MouseArea {
        id: selectorMouse
        hoverEnabled: true
        enabled: root.canSelect

        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.ArrowCursor
        onClicked: {
            root.forceActiveFocus();
            if (root.menuOpen)
                root.menuOpen = false;
            else
                root.openMenu();
        }
    }

    BarTooltip {
        anchorItem: root
        barWindow: root.parentWindow
        requested: selectorMouse.containsMouse
        text: root.Accessible.name + " · Super+Space"
    }

    Process {
        id: inputProcess

        onExited: function (exitCode) {
            if (exitCode === 0) {
                if (root.pendingSource)
                    Qt.callLater(root.dispatchSelection);
                else if (!root.keepOpenAfterSelection)
                    root.menuOpen = false;
            } else {
                root.pendingSource = null;
                root.lastError = "Could not change keyboard layout";
            }
        }
    }

    ShellPopup {
        id: inputMenu
        objectName: "keyboardLayoutPopup"
        keyboardInteractive: !root.cycling
        screen: root.parentWindow.screen
        anchorWindow: root.parentWindow
        anchorItem: root
        popupWidth: 260
        contentPadding: Theme.gap
        popupHeight: menuSurface.implicitHeight + contentPadding * 2
        onVisibleChanged: {
            if (visible) {
                root.opening();
                if (!root.cycling)
                    inputNavigation.focusMenu();
                inputNavigation.currentIndex = root.selectedIndex;
            }
        }

        MenuNavigator {
            id: inputNavigation
            entries: sourceRepeater
            focusTarget: menuSurface
            onEscapeRequested: root.menuOpen = false
            onActivateRequested: {
                if (entry && entry.sourceIndex >= 0 && entry.sourceIndex < root.sources.length)
                    root.selectSource(root.sources[entry.sourceIndex]);
            }
        }

        Connections {
            target: inputNavigation
            function onCurrentIndexChanged() {
                if (!inputNavigation.keyboardNavigation)
                    return;
                const entry = inputNavigation.currentEntry;
                if (entry && entry.sourceIndex >= 0)
                    root.setSelectedIndex(entry.sourceIndex, true);
            }
        }

        Rectangle {
            id: menuSurface
            objectName: "keyboardLayoutMenu"

            width: parent.width
            height: implicitHeight
            implicitHeight: menuColumn.implicitHeight
            color: "transparent"
            focus: !root.cycling
            Keys.onPressed: function (event) {
                if (event.key === Qt.Key_Space && (event.modifiers & Qt.MetaModifier)) {
                    root.cycleSource((event.modifiers & Qt.ShiftModifier) !== 0);
                    event.accepted = true;
                } else if (event.key === Qt.Key_Escape) {
                    inputNavigation.escapeRequested();
                    event.accepted = true;
                } else if (event.key === Qt.Key_Up) {
                    inputNavigation.move(-1);
                    event.accepted = true;
                } else if (event.key === Qt.Key_Down) {
                    inputNavigation.move(1);
                    event.accepted = true;
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Space) {
                    inputNavigation.activateCurrent();
                    event.accepted = true;
                }
            }

            Rectangle {
                readonly property var entry: sourceRepeater.count > root.selectedIndex && root.selectedIndex >= 0 ? sourceRepeater.itemAt(root.selectedIndex) : null
                x: 0
                y: entry ? menuColumn.y + entry.y : 0
                width: parent.width
                height: entry ? entry.height : 0
                radius: inputMenu.contentRadius
                color: Theme.selection
                visible: entry !== null
                Behavior on y {
                    enabled: inputMenu.visible && inputMenu.revealScale === 1 && !Theme.reducedMotion
                    NumberAnimation {
                        duration: Theme.motion
                        easing.type: Easing.OutCubic
                    }
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
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                    text: "Keyboard layout"
                    leftPadding: 8
                    rightPadding: 8
                    topPadding: 4
                    bottomPadding: 4
                }

                Repeater {
                    id: sourceRepeater
                    model: root.sources

                    delegate: Item {
                        id: sourceAction

                        required property var modelData
                        required property int index
                        readonly property bool menuEntry: true
                        readonly property int sourceIndex: index
                        Layout.fillWidth: true
                        Layout.preferredHeight: 38
                        Accessible.name: sourceAction.modelData.displayName
                        Accessible.role: Accessible.Button

                        Rectangle {
                            anchors.fill: parent
                            radius: inputMenu.contentRadius
                            color: sourceActionMouse.pressed ? Theme.pressed : sourceActionMouse.containsMouse ? Theme.hover : "transparent"
                        }

                        Text {
                            color: Theme.text
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                            textFormat: Text.PlainText
                            text: sourceAction.modelData.displayName

                            anchors {
                                left: parent.left
                                right: parent.right
                                leftMargin: 10
                                rightMargin: 34
                                verticalCenter: parent.verticalCenter
                            }
                        }

                        SymbolicIcon {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            implicitSize: 14
                            source: Quickshell.iconPath("object-select-symbolic")
                            color: Theme.accent
                            visible: root.metrics.currentInputSource !== null && root.sourceKey(sourceAction.modelData) === root.sourceKey(root.metrics.currentInputSource)
                        }

                        MouseArea {
                            id: sourceActionMouse

                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.ArrowCursor
                            enabled: root.canSelect
                            onClicked: {
                                inputNavigation.pointerActivate();
                                root.selectSource(sourceAction.modelData);
                            }
                        }

                        Keys.priority: Keys.BeforeItem
                        Keys.onDownPressed: function (event) {
                            inputNavigation.move(1);
                            event.accepted = true;
                        }
                        Keys.onUpPressed: function (event) {
                            inputNavigation.move(-1);
                            event.accepted = true;
                        }
                        Keys.onReturnPressed: function (event) {
                            inputNavigation.activateCurrent();
                            event.accepted = true;
                        }
                        Keys.onSpacePressed: function (event) {
                            inputNavigation.activateCurrent();
                            event.accepted = true;
                        }
                    }
                }

                Text {
                    Layout.fillWidth: true
                    visible: root.lastError !== ""
                    color: "#f4a340"
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
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
