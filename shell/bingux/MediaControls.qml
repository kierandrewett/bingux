import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets
import "MediaMatch.js" as MediaMatch

FocusScope {
    id: root
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
    property bool compact: false
    property bool artworkExpanded: false
    property real artExpansion: artworkExpanded ? 1 : 0
    readonly property real smallArtSize: compact ? 48 : Theme.controlHeight * 2
    readonly property real expandedArtSize: Math.max(0, width - Theme.padding * 2)
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
    implicitHeight: contents.implicitHeight + Theme.padding * 2 + expandedArtSpace + playerSelectorHeight
    Accessible.role: Accessible.Grouping
    Accessible.name: player ? player.identity + " playback" : "Media playback"

    Timer {
        interval: 1000
        repeat: true
        running: root.menuActive && root.hasPosition && root.player.isPlaying && !seek.pressed
        onTriggered: if (root.player) root.player.positionChanged()
    }
    Rectangle { objectName: "mediaCardSurface"; anchors.fill: parent; radius: root.cornerRadius; color: Theme.menuWidgetBackground }
    Item {
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
        objectName: "mediaArtButton"
        x: Theme.padding
        y: Theme.padding + root.playerSelectorHeight
        width: root.smallArtSize + (root.expandedArtSize - root.smallArtSize) * root.artExpansion
        height: width
        z: 2
        radius: Theme.insetRadius(root.cornerRadius, Theme.spaceSmall)
        color: Theme.shellSurface
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: root.artworkExpanded ? "Collapse album artwork" : "Expand album artwork"
        Accessible.onPressAction: root.artworkExpanded = !root.artworkExpanded
        Keys.onReturnPressed: root.artworkExpanded = !root.artworkExpanded
        Keys.onSpacePressed: root.artworkExpanded = !root.artworkExpanded
        AlbumArtwork {
            objectName: "mediaArtwork"
            anchors.fill: parent
            source: root.player ? root.player.trackArtUrl : ""
            active: root.menuActive
        }
        MouseArea {
            id: artMouse
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            hoverEnabled: true
            onClicked: root.artworkExpanded = !root.artworkExpanded
        }
        ShellTooltip { parent: artButton; visible: root.menuActive && artMouse.containsMouse; text: artButton.Accessible.name }
    }
    ColumnLayout {
        id: contents
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Theme.padding
        anchors.topMargin: Theme.padding + root.expandedArtSpace + root.playerSelectorHeight
        spacing: Theme.gap
        RowLayout {
            Layout.fillWidth: true
            spacing: 0
            Item {
                Layout.preferredWidth: (root.smallArtSize + Theme.gap * 2) * (1 - root.artExpansion)
                Layout.preferredHeight: root.smallArtSize * (1 - root.artExpansion)
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceSmall
                MarqueeText {
                    objectName: "mediaTitle"
                    Layout.fillWidth: true
                    text: root.player ? root.player.trackTitle || root.player.identity : "Not Playing"
                    textFormat: Text.PlainText
                    active: root.menuActive
                    color: Theme.text
                    pixelSize: root.compact ? Theme.fontSize : Theme.fontHeading
                    fontWeight: Font.DemiBold
                }
                Text {
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
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            Item {
                Layout.fillWidth: true
                Layout.preferredWidth: 0
                implicitHeight: Theme.controlHeight
                ActionButton {
                    id: elapsedTime
                    objectName: "mediaElapsed"
                    width: implicitWidth
                    implicitWidth: contentItem.implicitWidth + Theme.gap
                    padding: Theme.spaceSmall
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
                    contentItem: Text {
                        text: elapsedTime.text
                        verticalAlignment: Text.AlignVCenter
                        horizontalAlignment: Text.AlignLeft
                        color: Theme.muted
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
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
                text: root.seekButtons ? "Forward 10 seconds" : "Next track"
                iconName: root.seekButtons ? "media-seek-forward-symbolic" : "media-skip-forward-symbolic"
                direction: 1
                enabled: MediaMatch.canStep(root.player, 1)
                onClicked: if (enabled) { animateAction(); MediaMatch.step(root.player, 1); }
            }
            Text {
                Layout.fillWidth: true
                Layout.preferredWidth: 0
                horizontalAlignment: Text.AlignRight
                text: root.hasPosition ? MediaMatch.timeLabel(root.player.length) : ""
                color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall
            }
        }
    }
    component MediaButton: ActionButton {
        id: button
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
            visible: root.menuActive && (button.hovered || button.visualFocus)
            text: button.text
            x: (button.width - implicitWidth) / 2
            y: -implicitHeight - Theme.spaceSmall
        }
        implicitWidth: Theme.controlHeight + Theme.gap
        implicitHeight: Theme.controlHeight + Theme.gap
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
