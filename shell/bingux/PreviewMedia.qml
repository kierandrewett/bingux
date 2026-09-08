import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtMultimedia
import Quickshell

Item {
    id: root
    objectName: "previewMedia"
    property var details: null
    property real zoom: 1
    readonly property bool animation: details !== null && details.kind === "animation"
    property bool animationPaused: Theme.reducedMotion
    readonly property bool playing: animation ? !animationPaused : player.playbackState === MediaPlayer.PlayingState
    property bool controlsAwake: true
    readonly property bool controlsShown: controlsAwake || !playing || transportHover.hovered || seek.pressed || playPause.visualFocus || seek.visualFocus || muteButton.visualFocus
    property real reveal: 0
    Component.onCompleted: { reveal = 1; hideControls.restart(); }
    opacity: reveal
    Behavior on reveal { NumberAnimation { duration: Theme.previewMotion } }
    function timestamp(milliseconds) {
        const seconds = Math.floor(milliseconds / 1000);
        return Math.floor(seconds / 60) + ":" + String(seconds % 60).padStart(2, "0");
    }
    function wakeControls() { controlsAwake = true; hideControls.restart(); }
    function togglePlayback() {
        if (animation) animationPaused = !animationPaused;
        else if (playing) player.pause();
        else player.play();
        wakeControls();
    }
    Timer { id: hideControls; interval: 1800; onTriggered: root.controlsAwake = false }
    HoverHandler { onPointChanged: root.wakeControls() }
    MediaPlayer {
        id: player
        source: root.details && !root.animation ? root.details.source : ""
        autoPlay: !Theme.reducedMotion
        videoOutput: video
        audioOutput: AudioOutput { id: sound; muted: true }
    }
    component TransportButton: AbstractButton {
        id: button
        property string iconName: ""
        implicitWidth: 30
        implicitHeight: 30
        Accessible.name: text
        scale: down ? 0.9 : 1
        Behavior on scale { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
        background: Rectangle {
            radius: 7
            color: button.down ? "#40ffffff" : button.hovered || button.visualFocus ? "#20ffffff" : "transparent"
        }
        contentItem: Item {
            SymbolicIcon { anchors.centerIn: parent; implicitSize: 16; source: Quickshell.iconPath(button.iconName); color: Theme.muted }
        }
        ToolTip.visible: hovered
        ToolTip.delay: 600
        ToolTip.text: text
    }
    Flickable {
        id: viewport
        anchors.fill: parent
        clip: true
        contentWidth: width * root.zoom
        contentHeight: height * root.zoom
        boundsBehavior: Flickable.StopAtBounds
        onContentWidthChanged: contentX = Math.max(0, (contentWidth - width) / 2)
        onContentHeightChanged: contentY = Math.max(0, (contentHeight - height) / 2)
        Item {
            width: Math.max(viewport.width, viewport.contentWidth)
            height: Math.max(viewport.height, viewport.contentHeight)
            VideoOutput {
                id: video
                anchors.centerIn: parent
                width: viewport.contentWidth
                height: viewport.contentHeight
                visible: !root.animation && root.details && root.details.kind === "video"
                fillMode: VideoOutput.PreserveAspectFit
            }
            AnimatedImage {
                id: gif
                objectName: "previewAnimatedImage"
                anchors.centerIn: parent
                width: viewport.contentWidth
                height: viewport.contentHeight
                source: root.animation ? root.details.source : ""
                playing: root.animation && root.visible && !root.animationPaused
                fillMode: Image.PreserveAspectFit
                cache: false
                opacity: status === Image.Ready ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.previewMotion } }
            }
            Image {
                anchors.centerIn: parent
                width: viewport.contentWidth
                height: viewport.contentHeight
                source: root.details && root.details.poster ? root.details.poster : ""
                fillMode: Image.PreserveAspectFit
                opacity: root.animation ? (gif.status === Image.Ready ? 0 : 1) : (player.position > 0 ? 0 : 1)
                Behavior on opacity { NumberAnimation { duration: Theme.previewMotion } }
            }
            SymbolicIcon {
                anchors.centerIn: parent
                visible: root.details !== null && root.details.kind === "audio"
                implicitSize: 64
                source: Quickshell.iconPath("audio-x-generic-symbolic")
                color: Theme.muted
            }
            TapHandler { onTapped: root.togglePlayback() }
        }
        ScrollBar.vertical: ScrollBar {}
        ScrollBar.horizontal: ScrollBar {}
        PreviewSpinner {
            anchors.centerIn: parent
            loading: root.animation ? gif.status === Image.Loading : player.mediaStatus === MediaPlayer.LoadingMedia || player.mediaStatus === MediaPlayer.BufferingMedia
        }
        Text {
            anchors.centerIn: parent
            width: parent.width - 32
            visible: player.error !== MediaPlayer.NoError || gif.status === Image.Error
            text: root.animation ? "This animation could not be loaded." : player.errorString
            color: Theme.text
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }
    }
    Rectangle {
        id: transport
        objectName: "previewMediaTransport"
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 10
        width: root.animation ? 46 : parent.width - 20
        height: 42
        radius: 10
        color: "#e6202227"
        border.width: 1
        border.color: "#25ffffff"
        opacity: root.controlsShown ? 1 : 0
        transform: Translate { y: (1 - transport.opacity) * 6 }
        Behavior on opacity { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
        enabled: opacity > 0.5
        HoverHandler { id: transportHover }
        RowLayout {
            anchors.fill: parent
            anchors.margins: 6
            spacing: 6
            TransportButton {
                id: playPause
                objectName: "previewMediaPlayPause"
                text: root.playing ? "Pause" : "Play"
                contentItem: Item { PlayPauseGlyph { anchors.centerIn: parent; width: 18; height: 18; playing: root.playing; color: Theme.muted } }
                onClicked: root.togglePlayback()
            }
            Text {
                visible: !root.animation
                text: root.timestamp(seek.pressed ? seek.value : player.position)
                color: "#eeeeef"
                font.family: Theme.fontFamily
                font.pixelSize: 10
                Layout.preferredWidth: 31
                horizontalAlignment: Text.AlignRight
            }
            Slider {
                id: seek
                objectName: "previewMediaSeek"
                Layout.fillWidth: true
                visible: !root.animation
                implicitHeight: 30
                from: 0
                to: player.duration || 1
                value: player.position
                enabled: player.seekable
                onMoved: { player.position = value; root.wakeControls(); }
                Accessible.name: "Playback position"
                background: Rectangle {
                    x: seek.leftPadding
                    y: seek.topPadding + seek.availableHeight / 2 - height / 2
                    width: seek.availableWidth
                    height: 3
                    radius: 2
                    color: "#40ffffff"
                    Rectangle { width: seek.visualPosition * parent.width; height: parent.height; radius: 2; color: Theme.searchAccent }
                }
                handle: Rectangle {
                    x: seek.leftPadding + seek.visualPosition * (seek.availableWidth - width)
                    y: seek.topPadding + seek.availableHeight / 2 - height / 2
                    width: 9; height: 9; radius: 5
                    color: Theme.muted
                    scale: seek.pressed ? 1.25 : seek.hovered ? 1 : 0.7
                    Behavior on scale { NumberAnimation { duration: Theme.previewMotion } }
                }
            }
            Text {
                visible: !root.animation
                text: root.timestamp(player.duration)
                color: "#bcbfc5"
                font.family: Theme.fontFamily
                font.pixelSize: 10
                Layout.preferredWidth: 31
            }
            TransportButton {
                id: muteButton
                visible: !root.animation
                text: sound.muted ? "Unmute" : "Mute"
                iconName: sound.muted ? "audio-volume-muted-symbolic" : "audio-volume-high-symbolic"
                onClicked: { sound.muted = !sound.muted; root.wakeControls(); }
            }
        }
    }
}
