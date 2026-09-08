import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets
import "MediaMatch.js" as MediaMatch

FocusScope {
    id: root
    objectName: "mediaControls"
    required property var player
    property var playerOptions: []
    signal playerSelected(var selectedPlayer)
    readonly property bool multiplePlayers: playerOptions.length > 1
    readonly property real playerSelectorHeight: multiplePlayers ? Theme.controlHeight + Theme.gap : 0
    function cyclePlayer(direction) {
        const index = playerOptions.indexOf(player);
        if (playerOptions.length) playerSelected(playerOptions[(Math.max(0, index) + direction + playerOptions.length) % playerOptions.length]);
    }
    property bool menuActive: false
    // Position is extrapolated by MPRIS, but bindings need a notification after
    // the hidden menu has stopped its display timer.
    onMenuActiveChanged: if (menuActive && player && player.positionSupported) player.positionChanged()
    property bool compact: false
    property bool barLayout: false
    property var barWindow: null
    property var presentation: null
    property string editWidgetId: ""
    signal editRequested(string widgetId, var control)
    readonly property bool inlineControls: barLayout && !details.retained
    readonly property alias detailPopup: details
    readonly property real cardHeight: contents.implicitHeight + Theme.padding * 2 + expandedArtSpace + playerSelectorHeight
    function toggleArtwork() {
        artworkExpanded = !artworkExpanded;
        if (barLayout) details.visible = true;
    }
    onBarLayoutChanged: if (!barLayout) details.visible = false
    onVisibleChanged: if (!visible) details.visible = false
    Connections { target: DesktopEditing; function onActiveChanged() { if (DesktopEditing.active) details.visible = false; } }
    Item {
        id: fullContent
        readonly property var editWindow: details.nativeWindow
        parent: root.barLayout ? details.body : root
        width: parent.width
        height: root.cardHeight
    }
    WidgetEditHandle {
        control: fullContent
        widgetId: root.editWidgetId
        previewSource: false
        visible: root.barLayout && root.editWidgetId !== ""
        onRequested: (id, item) => root.editRequested(id, item)
    }
    ShellPopup {
        id: details
        objectName: "mediaDetailsPopup"
        anchorWindow: root.barWindow
        screen: root.barWindow?.screen || Quickshell.screens[0]
        anchorItem: root
        contentPadding: 0
        popupWidth: 400
        popupHeight: root.cardHeight
        surfaceVisible: false
        cornerRadius: root.cornerRadius
    }
    GridLayout {
        id: inlineBar
        visible: root.inlineControls
        anchors.fill: parent
        rows: 1
        columnSpacing: Theme.gap
        rowSpacing: 0
    }
    ActionButton {
        id: summaryButton
        anchors.fill: parent
        alignLeft: true
        presentation: root.presentation
        background: BarControlSurface { hovered: summaryButton.hovered; pressed: summaryButton.down; selected: true; focused: summaryButton.visualFocus }
        visible: root.barLayout && !root.inlineControls
        text: root.player ? root.player.trackTitle || root.player.identity : "Media playback"
        onClicked: details.visible = !details.visible
    }
    property bool artworkExpanded: false
    property real artExpansion: artworkExpanded ? 1 : 0
    readonly property real smallArtSize: compact ? 48 : Theme.controlHeight * 2
    readonly property real expandedArtSize: Math.max(0, fullContent.width - Theme.padding * 2)
    readonly property real expandedArtSpace: (expandedArtSize + Theme.gap) * artExpansion
    readonly property color accentHover: Qt.rgba(accent.r + (1 - accent.r) * 0.16, accent.g + (1 - accent.g) * 0.16, accent.b + (1 - accent.b) * 0.16, 1)
    readonly property color accentPressed: Qt.rgba(accent.r + (1 - accent.r) * 0.08, accent.g + (1 - accent.g) * 0.08, accent.b + (1 - accent.b) * 0.08, 1)
    Behavior on artExpansion { NumberAnimation { duration: Theme.reducedMotion ? 0 : 280; easing.type: Easing.InOutCubic } }
    property color accent: Theme.accent
    property color accentForeground: Theme.shellSurface
    property real cornerRadius: Theme.menuWidgetRadius
    readonly property bool menuEntry: true
    readonly property bool controllable: player !== null && player.canControl
    readonly property bool hasPosition: player !== null && player.positionSupported && player.lengthSupported && player.length > 0
    readonly property real position: hasPosition ? Math.max(0, Math.min(player.position, player.length)) : 0
    readonly property bool seekButtons: MediaMatch.prefersSeeking(player)
    property int seekTrack: -1
    property bool showRemaining: false
    implicitWidth: !barLayout ? 0 : presentation && !presentation.showText
        ? (presentation.showIcon ? Theme.barHeight : 0) + Theme.barHeight * 3 + Theme.gap * 3 : 320
    implicitHeight: barLayout ? Theme.barHeight : cardHeight
    Accessible.role: Accessible.Grouping
    Accessible.name: player ? player.identity + " playback" : "Media playback"

    Timer {
        interval: 1000
        repeat: true
        running: root.menuActive && root.hasPosition && root.player.isPlaying && !seek.pressed
        onTriggered: if (root.player) root.player.positionChanged()
    }
    // Reparented controls must stay above the card when they return from the bar.
    Rectangle { parent: fullContent; z: -1; objectName: "mediaCardSurface"; anchors.fill: parent; radius: root.cornerRadius; color: Theme.menuWidgetBackground }
    Item {
        parent: fullContent
        objectName: "mediaPlayerSelector"
        visible: root.multiplePlayers
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: Theme.padding
        height: Theme.controlHeight
        IconButton {
            id: previousPlayerButton
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            objectName: "mediaPreviousPlayer"
            iconName: "go-previous-symbolic"
            label: "Previous media player"
            onClicked: root.cyclePlayer(-1)
        }
        RowLayout {
            id: playerIdentity
            objectName: "mediaPlayerIdentity"
            anchors.centerIn: parent
            width: Math.min(implicitWidth, Math.max(0, parent.width - 2 * (nextPlayerButton.width + playerCount.implicitWidth + Theme.gap * 2)))
            spacing: Theme.gap
            readonly property var desktopEntry: MediaMatch.playerDesktopEntry(root.player, DesktopEntries)
            OsIconImage {
                objectName: "mediaPlayerAppIcon"
                Layout.preferredWidth: Theme.iconSize
                Layout.preferredHeight: Theme.iconSize
                implicitSize: Theme.iconSize
                source: playerIdentity.desktopEntry && playerIdentity.desktopEntry.icon ? playerIdentity.desktopEntry.icon : "applications-multimedia"
            }
            Text {
                objectName: "mediaPlayerName"
                Layout.fillWidth: true
                text: root.player ? root.player.identity : "Media players"
                textFormat: Text.PlainText
                elide: Text.ElideRight
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
        }
        Text {
            id: playerCount
            anchors.right: nextPlayerButton.left
            anchors.rightMargin: Theme.gap
            anchors.verticalCenter: parent.verticalCenter
            text: (Math.max(0, root.playerOptions.indexOf(root.player)) + 1) + " / " + root.playerOptions.length
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        IconButton {
            id: nextPlayerButton
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            objectName: "mediaNextPlayer"
            iconName: "go-next-symbolic"
            label: "Next media player"
            onClicked: root.cyclePlayer(1)
        }
    }
    ClippingRectangle {
        id: artButton
        parent: root.inlineControls ? root : fullContent
        visible: !root.inlineControls || !root.presentation || root.presentation.showIcon
        objectName: "mediaArtButton"
        x: root.inlineControls ? 0 : Theme.padding
        y: root.inlineControls ? (root.height - height) / 2 : Theme.padding + root.playerSelectorHeight
        width: root.inlineControls ? Theme.barHeight - 4 : root.smallArtSize + (root.expandedArtSize - root.smallArtSize) * root.artExpansion
        height: width
        z: 2
        radius: Theme.insetRadius(root.cornerRadius, Theme.spaceSmall)
        color: Theme.shellSurface
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: root.artworkExpanded ? "Collapse album artwork" : "Expand album artwork"
        Accessible.onPressAction: root.toggleArtwork()
        Keys.onReturnPressed: root.toggleArtwork()
        Keys.onSpacePressed: root.toggleArtwork()
        AlbumArtwork {
            objectName: "mediaArtwork"
            visible: !root.inlineControls || !root.presentation?.iconOverridden
            anchors.fill: parent
            source: root.player ? root.player.trackArtUrl : ""
            active: root.menuActive
        }
        SymbolicIcon {
            objectName: "mediaOverrideIcon"
            anchors.centerIn: parent
            visible: root.inlineControls && !!root.presentation?.iconOverridden
            source: visible ? Quickshell.iconPath(root.presentation.icon) : ""
            color: Theme.text
        }
        MouseArea {
            id: artMouse
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            hoverEnabled: true
            onClicked: root.toggleArtwork()
        }
        BarTooltip { anchorItem: artButton; barWindow: root.barWindow; requested: root.inlineControls && root.menuActive && artMouse.containsMouse; text: artButton.Accessible.name }
        ShellTooltip { parent: artButton; visible: !root.inlineControls && root.menuActive && artMouse.containsMouse; text: artButton.Accessible.name }
    }
    GridLayout {
        id: contents
        parent: fullContent
        columns: 1
        columnSpacing: 0
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Theme.padding
        anchors.topMargin: Theme.padding + root.expandedArtSpace + root.playerSelectorHeight
        rowSpacing: Theme.gap
        RowLayout {
            id: titleRow
            parent: root.inlineControls ? inlineBar : contents
            Layout.row: 0
            Layout.column: 0
            Layout.fillWidth: true
            spacing: 0
            MouseArea {
                objectName: "mediaOpenDetails"
                parent: root
                x: root.inlineControls ? titleRow.x : 0
                y: root.inlineControls ? titleRow.y : 0
                width: root.inlineControls ? titleRow.width : 0
                height: root.inlineControls ? titleRow.height : 0
                enabled: root.inlineControls
                activeFocusOnTab: root.inlineControls
                Accessible.role: Accessible.Button
                Accessible.name: "Open media controls"
                Accessible.onPressAction: details.visible = true
                Keys.onReturnPressed: details.visible = true
                Keys.onSpacePressed: details.visible = true
                cursorShape: Qt.PointingHandCursor
                onClicked: details.visible = true
            }
            Item {
                Layout.preferredWidth: root.inlineControls ? (artButton.visible ? artButton.width + Theme.gap : 0) : (root.smallArtSize + Theme.gap * 2) * (1 - root.artExpansion)
                Layout.preferredHeight: root.inlineControls ? Theme.barHeight : root.smallArtSize * (1 - root.artExpansion)
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceSmall
                MarqueeText {
                    objectName: "mediaTitle"
                    Layout.fillWidth: true
                    visible: !root.inlineControls || !root.presentation || root.presentation.showText
                    text: root.inlineControls && root.presentation?.custom ? root.presentation.label : root.player ? root.player.trackTitle || root.player.identity : "Not Playing"
                    textFormat: Text.PlainText
                    active: root.menuActive
                    color: Theme.text
                    pixelSize: root.compact ? Theme.fontSize : Theme.fontHeading
                    fontWeight: Font.DemiBold
                }
                Text {
                    visible: !root.inlineControls
                    Layout.fillWidth: true
                    text: root.player ? root.player.trackArtist || root.player.identity : "Music and audio"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
            }
        }
        SeekSlider {
            id: seek
            Layout.row: 1
            accent: root.accent
            trackPreviewVisible: seek.previewVisible
            trackPreviewPosition: seek.previewPosition
            objectName: "mediaSeek"
            Layout.fillWidth: true
            Layout.preferredHeight: Theme.sliderControlHeight
            visible: root.hasPosition
            enabled: root.controllable && root.hasPosition && root.player.canSeek
            from: 0
            to: root.hasPosition ? root.player.length : 1
            Binding { target: seek; property: "value"; value: seek.wheelActive ? seek.wheelTarget : root.position; when: !seek.pressed; delayed: true }
            property bool wheelActive: false
            property real wheelTarget: 0
            property int wheelTrack: -1
            Behavior on value {
                enabled: seek.wheelActive && !seek.pressed && !Theme.reducedMotion
                NumberAnimation { duration: Theme.mediaActionMotion; easing.type: Easing.OutCubic }
            }
            Timer {
                id: wheelCommit
                interval: 60
                onTriggered: if (seek.enabled && root.player && root.player.uniqueId === seek.wheelTrack)
                    root.player.position = seek.wheelTarget;
            }
            Timer { id: wheelFinish; interval: Theme.mediaActionMotion + 120; onTriggered: seek.wheelActive = false }
            live: false
            stepSize: 1
            wheelEnabled: false
            hoverEnabled: true
            leftPadding: 0
            rightPadding: 0
            readonly property real previewPosition: pressed ? position : normalizedPositionAt(seekHover.point.position.x)
            readonly property bool previewVisible: enabled && (seekHover.hovered || pressed)
            HoverHandler { id: seekHover; cursorShape: Qt.PointingHandCursor }
            WheelHandler {
                enabled: seek.enabled && !seek.pressed
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                property real remainder: 0
                onWheel: event => {
                    remainder += event.angleDelta.y !== 0 ? event.angleDelta.y / 120 : event.pixelDelta.y / 40;
                    const steps = Math.trunc(remainder);
                    if (steps !== 0) {
                        remainder -= steps;
                        const start = seek.wheelActive && seek.wheelTrack === root.player.uniqueId ? seek.wheelTarget : root.player.position;
                        seek.wheelTrack = root.player.uniqueId;
                        seek.wheelActive = true;
                        seek.wheelTarget = Math.max(0, Math.min(seek.to, start + steps * 5));
                        wheelCommit.restart();
                        wheelFinish.restart();
                    }
                    event.accepted = true;
                }
            }
            ShellTooltip {
                objectName: "mediaSeekPreview"
                parent: seek
                visible: root.menuActive && seek.previewVisible
                delay: 0
                timeout: -1
                text: MediaMatch.timeLabel(seek.previewPosition * seek.to)
                x: Math.max(0, Math.min(seek.width - implicitWidth, seek.leftPadding + seek.handle.width / 2 + (seek.mirrored ? 1 - seek.previewPosition : seek.previewPosition) * (seek.availableWidth - seek.handle.width) - implicitWidth / 2))
                y: -implicitHeight - Theme.spaceSmall
            }
            Accessible.name: "Playback position"
            onPressedChanged: {
                if (pressed && root.player) {
                    wheelCommit.stop(); wheelFinish.stop(); wheelActive = false;
                    root.seekTrack = root.player.uniqueId;
                }
                else if (enabled && root.seekTrack === root.player.uniqueId) root.player.position = value;
            }
            Keys.onPressed: function(event) { root.seekTrack = -1; event.accepted = false; }
            onMoved: {
                if (enabled && !pressed && root.seekTrack < 0)
                    root.player.position = value;
            }

        }
        GridLayout {
            id: transport
            Layout.row: 2
            Layout.fillWidth: true
            rows: 1
            columnSpacing: Theme.gap
            rowSpacing: 0
            Item {
                Layout.column: 0
                Layout.fillWidth: true
                Layout.preferredWidth: 0
                implicitHeight: Theme.controlHeight
                ActionButton {
                    id: elapsedTime
                    objectName: "mediaElapsed"
                    width: implicitWidth
                    implicitWidth: elapsedDigits.implicitWidth + horizontalPadding * 2
                    padding: Theme.spaceSmall
                    horizontalPadding: Theme.spaceSmall
                    implicitHeight: Theme.controlHeight
                    enabled: root.hasPosition
                    hoverEnabled: true
                    activeFocusOnTab: true
                    showFocusRing: visualFocus
                    readonly property real displayedPosition: seek.pressed ? seek.position * seek.to : root.position
                    text: !root.hasPosition ? "" : root.showRemaining
                        ? "-" + MediaMatch.timeLabel(Math.max(0, seek.to - displayedPosition)) : MediaMatch.timeLabel(displayedPosition)
                    Accessible.name: (root.showRemaining ? "Time remaining " : "Elapsed time ") + text
                    onClicked: root.showRemaining = !root.showRemaining
                    background: ControlCentreButtonSurface { control: elapsedTime; animated: false; radius: Theme.gap; baseColor: "transparent" }
                    contentItem: RollingNumber {
                        id: elapsedDigits
                        objectName: "mediaElapsedDigits"
                        duration: 320
                        text: elapsedTime.text
                        value: root.showRemaining ? seek.to - elapsedTime.displayedPosition : elapsedTime.displayedPosition
                        motionEnabled: root.menuActive && !seek.pressed
                        color: Theme.muted
                        pixelSize: Theme.fontSmall
                        fontWeight: Font.Normal
                    }
                    ShellTooltip {
                        parent: elapsedTime
                        visible: root.menuActive && (elapsedTime.hovered || elapsedTime.visualFocus)
                        text: root.showRemaining ? "Show elapsed time" : "Show time remaining"
                    }
                }
            }
            MediaButton {
                objectName: "mediaPrevious"
                Layout.column: 1
                text: root.seekButtons ? "Back 10 seconds" : "Previous track"
                iconName: root.seekButtons ? "media-seek-backward-symbolic" : "media-skip-backward-symbolic"
                direction: -1
                enabled: MediaMatch.canStep(root.player, -1)
                onClicked: if (enabled) { animateAction(); MediaMatch.step(root.player, -1); }
            }
            MediaButton {
                id: playPause
                primary: true
                objectName: "mediaPlayPause"
                Layout.column: 2
                focus: true
                text: root.player && root.player.isPlaying ? "Pause" : "Play"
                iconName: root.player && root.player.isPlaying ? "media-playback-pause-symbolic" : "media-playback-start-symbolic"
                enabled: root.controllable && (root.player.isPlaying ? root.player.canPause : root.player.canPlay)
                onClicked: {
                    if (!enabled) return;
                    if (root.player.isPlaying) root.player.pause();
                    else root.player.play();
                }
            }
            MediaButton {
                objectName: "mediaNext"
                Layout.column: 3
                text: root.seekButtons ? "Forward 10 seconds" : "Next track"
                iconName: root.seekButtons ? "media-seek-forward-symbolic" : "media-skip-forward-symbolic"
                direction: 1
                enabled: MediaMatch.canStep(root.player, 1)
                onClicked: if (enabled) { animateAction(); MediaMatch.step(root.player, 1); }
            }
            Item {
                Layout.column: 4
                Layout.fillWidth: true
                Layout.preferredWidth: 0
                implicitHeight: totalTime.implicitHeight
                RollingNumber {
                    id: totalTime
                    objectName: "mediaDurationDigits"
                    duration: 320
                    anchors.right: parent.right
                    text: root.hasPosition ? MediaMatch.timeLabel(root.player.length) : ""
                    value: root.hasPosition ? root.player.length : 0
                    motionEnabled: root.menuActive
                    color: Theme.muted
                    pixelSize: Theme.fontSmall
                    fontWeight: Font.Normal
                }
            }
        }
    }
    component MediaButton: ActionButton {
        id: button
        parent: root.inlineControls ? inlineBar : transport
        Layout.row: 0
        required property string iconName
        property bool primary: false
        property int direction: 0
        property string displayedIcon: iconName
        property real iconOffset: 0
        property real iconOpacity: 1
        onIconNameChanged: {
            actionMotion.stop();
            displayedIcon = iconName;
            iconOffset = 0;
            iconOpacity = 1;
        }
        function animateAction() {
            actionMotion.stop();
            iconOffset = 0;
            iconOpacity = 1;
            if (Theme.reducedMotion) { displayedIcon = iconName; return; }
            actionMotion.start();
        }
        SequentialAnimation {
            id: actionMotion
            ParallelAnimation {
                NumberAnimation { target: button; property: "iconOpacity"; to: 0; duration: Theme.mediaActionMotion / 2 }
                NumberAnimation { target: button; property: "iconOffset"; to: button.direction * Theme.spaceSmall; duration: Theme.mediaActionMotion / 2 }
            }
            ScriptAction { script: { button.displayedIcon = button.iconName; button.iconOffset = -button.direction * Theme.spaceSmall; } }
            ParallelAnimation {
                NumberAnimation { target: button; property: "iconOpacity"; to: 1; duration: Theme.mediaActionMotion / 2 }
                NumberAnimation { target: button; property: "iconOffset"; to: 0; duration: Theme.mediaActionMotion / 2 }
            }
        }
        hoverEnabled: true
        ShellTooltip {
            parent: button
            visible: !root.inlineControls && root.menuActive && (button.hovered || button.visualFocus)
            text: button.text
            x: (button.width - implicitWidth) / 2
            y: -implicitHeight - Theme.spaceSmall
        }
        BarTooltip { anchorItem: button; barWindow: root.barWindow; requested: root.inlineControls && root.menuActive && (button.hovered || button.visualFocus); text: button.text }
        implicitWidth: root.inlineControls ? Theme.barHeight : Theme.controlHeight + Theme.gap
        implicitHeight: implicitWidth
        cornerRadius: implicitHeight / 2
        background: ControlCentreButtonSurface {
            control: button
            animated: false
            hoverColor: button.primary ? root.accentHover : Theme.hover
            pressedColor: button.primary ? root.accentPressed : Theme.pressed
            radius: button.cornerRadius
            baseColor: button.primary ? root.accent : "transparent"
            focusColor: button.primary ? Theme.text : root.accent
            opacity: button.enabled ? 1 : 0.4
        }
        contentItem: Item {
            SymbolicIcon {
                visible: !button.primary
                anchors.centerIn: parent
                anchors.horizontalCenterOffset: button.iconOffset
                opacity: button.iconOpacity
                source: Quickshell.iconPath(button.displayedIcon)
                color: button.primary ? root.accentForeground : button.enabled ? Theme.text : Theme.muted
                implicitSize: Theme.iconSize
            }
            PlayPauseGlyph {
                objectName: "mediaPlayPauseGlyph"
                width: Theme.iconSize + Theme.gap
                height: width
                anchors.centerIn: parent
                visible: button.primary
                playing: !!root.player && root.player.isPlaying
                color: root.accentForeground
            }
        }
    }
}
