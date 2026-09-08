import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell.Services.Mpris

ScrollView {
    id: root
    property var players: Mpris.players.values
    contentWidth: availableWidth
    clip: true
    ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
    function focusContent() { forceActiveFocus(); }
    ColumnLayout {
        width: root.availableWidth
        spacing: Theme.gap
        Repeater {
            model: root.players
            ColumnLayout {
                id: entry
                required property var modelData
                required property int index
                readonly property var player: modelData
                objectName: "sidebarPlayer" + index
                Layout.fillWidth: true
                spacing: Theme.gap
                Text {
                    visible: root.players.length > 1
                    Layout.fillWidth: true
                    Layout.leftMargin: 4
                    text: entry.player ? entry.player.identity : ""
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    color: Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
                MediaControls {
                    visible: entry.player !== null && root.availableWidth >= 260
                    Layout.fillWidth: true
                    player: entry.player
                    menuActive: root.visible && visible
                    compact: true
                    artworkExpanded: !!entry.player && entry.player.trackArtUrl.length > 0
                }
                ColumnLayout {
                    visible: entry.player !== null && root.availableWidth < 260
                    Layout.fillWidth: true
                    AlbumArtwork { Layout.fillWidth: true; Layout.preferredHeight: width; source: entry.player ? entry.player.trackArtUrl : ""; active: root.visible }
                    Text { Layout.fillWidth: true; text: entry.player ? entry.player.trackTitle || entry.player.identity : ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize }
                    Text { Layout.fillWidth: true; text: entry.player ? entry.player.trackArtist : ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSmall }
                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        spacing: 2
                        IconButton { iconName: "media-skip-backward-symbolic"; label: "Previous track"; enabled: !!entry.player && entry.player.canGoPrevious; onClicked: entry.player.previous() }
                        IconButton { iconName: entry.player && entry.player.isPlaying ? "media-playback-pause-symbolic" : "media-playback-start-symbolic"; label: "Play or pause"; enabled: !!entry.player && entry.player.canControl && (entry.player.isPlaying ? entry.player.canPause : entry.player.canPlay); onClicked: entry.player.isPlaying ? entry.player.pause() : entry.player.play() }
                        IconButton { iconName: "media-skip-forward-symbolic"; label: "Next track"; enabled: !!entry.player && entry.player.canGoNext; onClicked: entry.player.next() }
                    }
                }
            }
        }
        Text {
            visible: root.players.length === 0
            Layout.fillWidth: true
            Layout.topMargin: 24
            text: "Play music or a video in an app to control it here."
            wrapMode: Text.Wrap
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
    }
}
