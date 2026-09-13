import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import QtQml.Models
import Quickshell

ShellPopup {
    id: root
    property var menu: null
    property var actions: null
    property bool compactRows: false
    readonly property var menuEntries: root.normaliseEntries(actions !== null ? actions : opener.children.values)
    readonly property bool hasMenuItems: root.menuEntries.some(entry => entry && !entry.isSeparator)
    property var parents: []
    property var currentMenu: menu
    property var transitionFromEntries: []
    property var transitionToEntries: []
    property real pageProgress: 1
    property int pageDirection: 1
    property bool pageTransitioning: false
    property bool pageCommitReady: false
    property int pageCommitAttempts: 0
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
    popupHeight: Math.min(620, Math.max(entries.contentHeight, outgoingEntries.contentHeight) + contentPadding * 2 + (parents.length ? 40 : 0))
    onMenuChanged: {
        parents = [];
        currentMenu = menu;
        transitionFromEntries = [];
        transitionToEntries = [];
        pageProgress = 1;
        pageTransitioning = false;
        pageCommitReady = false;
        pageCommitAttempts = 0;
    }
    onVisibleChanged: {
        if (visible) {
            navigation.focusMenu();
            return;
        }
        // Do not let a closing menu keep an animation alive while its native
        // QsMenu handle is being released by the tray item.
        pageCommit.stop();
        pageAnimation.stop();
        pageTransitioning = false;
        pageCommitReady = false;
        pageCommitAttempts = 0;
        transitionFromEntries = [];
        transitionToEntries = [];
    }
    onHasMenuItemsChanged: if (!hasMenuItems)
        visible = false;
    // Populate menu entries while the tray icon is hovered, before a click
    // needs to show the menu surface.
    QsMenuOpener {
        id: opener
        menu: root.actions !== null ? null : root.currentMenu
    }
    Connections {
        target: opener
        function onChildrenChanged() {
            root.refreshTransitionEntries();
        }
    }
    Connections {
        // A provider can fill the ObjectModel after QsMenuOpener has emitted
        // childrenChanged. Listen to the model itself as well, otherwise a
        // submenu can commit its Back button before its rows arrive.
        target: opener.children
        function onValuesChanged() {
            root.refreshTransitionEntries();
        }
    }
    MenuNavigator {
        id: navigation
        entries: root.menuEntries
        view: entries
        focusTarget: entries
        onEscapeRequested: root.visible = false
        onActivateRequested: root.activate(entry)
    }
    Instantiator {
        id: menuTextMetrics
        model: root.menuEntries
        delegate: TextMetrics {
            required property var modelData
            text: modelData && typeof modelData.text === "string" ? modelData.text.replace(/&(.)/g, "$1") : ""
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
    }
    function back() {
        if (!parents.length) {
            visible = false;
            return;
        }
        const path = parents.slice();
        const target = path.pop();
        root.showMenu(target, -1, path);
    }

    // QsMenu's UntypedObjectModel can expose a null row for one frame while a
    // native menu handle is being replaced. Keep that transient state out of
    // every view and take plain metadata snapshots for the animation so old
    // delegates never bind directly to entries that have just been destroyed.
    function normaliseEntries(values) {
        if (!values)
            return [];
        const result = [];
        for (let index = 0; index < values.length; index++) {
            if (values[index])
                result.push(values[index]);
        }
        return result;
    }

    function snapshotEntries(values, includeEntry) {
        return root.normaliseEntries(values).map(entry => ({
                    entry: includeEntry ? entry : null,
                    text: typeof entry.text === "string" ? entry.text : "",
                    enabled: !!entry.enabled,
                    isSeparator: !!entry.isSeparator,
                    hasChildren: !!entry.hasChildren,
                    checkState: entry.checkState
                }));
    }

    function refreshTransitionEntries() {
        if (!root.pageTransitioning)
            return;
        if (root.pageCommitReady)
            root.transitionToEntries = root.snapshotEntries(root.menuEntries, true);
        else
            pageCommit.restart();
    }

    function activate(entry) {
        const action = entry && entry.entry ? entry.entry : entry;
        if (!action || !action.enabled || action.isSeparator)
            return;
        if (action.hasChildren) {
            root.showMenu(action, 1, parents.concat([currentMenu]));
        } else {
            if (typeof action.triggered !== "function")
                return;
            action.triggered();
            visible = false;
        }
    }
    function showMenu(target, direction, path) {
        if (!target)
            return;
        transitionFromEntries = root.snapshotEntries(root.menuEntries, false);
        transitionToEntries = [];
        pageDirection = direction < 0 ? -1 : 1;
        pageProgress = 0;
        pageTransitioning = true;
        pageCommitReady = false;
        pageCommitAttempts = 0;
        parents = path || [];
        currentMenu = target;
        pageCommit.restart();
    }

    Timer {
        id: pageCommit
        interval: 24
        onTriggered: {
            const nextEntries = root.snapshotEntries(root.menuEntries, true);
            const waitingForNativeChildren = root.actions === null && root.currentMenu && root.currentMenu.hasChildren && nextEntries.length === 0;
            if (waitingForNativeChildren && root.pageCommitAttempts++ < 40) {
                restart();
                return;
            }
            transitionToEntries = nextEntries;
            pageCommitReady = true;
            pageAnimation.restart();
        }
    }

    NumberAnimation {
        id: pageAnimation
        target: root
        property: "pageProgress"
        from: 0
        to: 1
        duration: Theme.reducedMotion ? 0 : 160
        easing.type: Easing.OutCubic
        onFinished: {
            root.pageProgress = 1;
            root.pageTransitioning = false;
            root.pageCommitReady = false;
            root.pageCommitAttempts = 0;
            root.transitionFromEntries = [];
            navigation.focusMenu();
        }
    }

    Component {
        id: menuEntryDelegate
        ItemDelegate {
            id: entryButton
            required property var modelData
            required property int index
            readonly property var owner: ListView.view
            readonly property var data: modelData || ({})
            objectName: "trayMenuEntry" + index
            width: owner ? owner.width : 0
            height: data.isSeparator ? 9 : 38
            enabled: !root.pageTransitioning && !!modelData && data.enabled && !data.isSeparator
            text: data.text.replace(/&(.)/g, "$1")
            leftPadding: root.compactRows ? 10 : 12
            rightPadding: root.compactRows ? 10 : 12
            Accessible.name: text
            onClicked: {
                navigation.pointerActivate();
                root.activate(data);
            }
            background: Rectangle {
                radius: root.contentRadius
                color: entryButton.down ? Theme.pressed : entryButton.enabled && (entryButton.hovered || (root.keyboardNavigation && entryButton.owner && entryButton.owner.activeFocus && entryButton.owner.currentIndex === entryButton.index)) ? Theme.hover : "transparent"
                Rectangle {
                    visible: entryButton.data.isSeparator
                    anchors.centerIn: parent
                    width: parent.width - 12
                    height: 1
                    color: Theme.outline
                }
            }
            contentItem: RowLayout {
                visible: !!entryButton.modelData && !entryButton.data.isSeparator
                spacing: Theme.gap
                Text {
                    visible: !root.compactRows
                    Layout.preferredWidth: 18
                    text: entryButton.data.checkState === Qt.Checked ? "✓" : ""
                    color: Theme.accent
                }
                Text {
                    Layout.fillWidth: true
                    text: entryButton.text
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: entryButton.enabled ? Theme.text : Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                }
                SymbolicIcon {
                    visible: root.compactRows && entryButton.data.checkState === Qt.Checked
                    implicitSize: 14
                    source: Quickshell.iconPath("object-select-symbolic")
                    color: Theme.accent
                }
                Text {
                    text: entryButton.data.hasChildren ? "›" : ""
                    color: Theme.muted
                }
            }
        }
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
        Item {
            id: pageViewport
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            ListView {
                id: outgoingEntries
                anchors.fill: parent
                model: root.transitionFromEntries
                spacing: 2
                visible: root.pageTransitioning
                x: -root.pageDirection * root.pageProgress * width
                opacity: 1 - root.pageProgress
                boundsBehavior: Flickable.StopAtBounds
                delegate: menuEntryDelegate
            }
            ListView {
                id: entries
                anchors.fill: parent
                model: root.pageTransitioning ? root.transitionToEntries : root.menuEntries
                spacing: 2
                x: root.pageTransitioning ? root.pageDirection * (1 - root.pageProgress) * width : 0
                opacity: root.pageTransitioning ? root.pageProgress : 1
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar {}
                Keys.onDownPressed: function (event) {
                    navigation.move(1);
                    event.accepted = true;
                }
                Keys.onUpPressed: function (event) {
                    navigation.move(-1);
                    event.accepted = true;
                }
                Keys.onReturnPressed: function (event) {
                    navigation.activateCurrent();
                    event.accepted = true;
                }
                Keys.onSpacePressed: function (event) {
                    navigation.activateCurrent();
                    event.accepted = true;
                }
                Keys.onLeftPressed: root.back()
                Keys.onRightPressed: if (currentItem && currentItem.modelData && currentItem.modelData.hasChildren)
                    navigation.activateCurrent()
                delegate: menuEntryDelegate
            }
        }
    }
}
