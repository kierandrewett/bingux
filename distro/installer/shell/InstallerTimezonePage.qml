import QtQuick
import Quickshell.Io

// Time zone picked on a world map: a click selects the nearest zone from the
// zone1970.tab coordinates (assets/timezones.json). The map is an equirectangular
// Natural Earth land layer (assets/world-map.svg, lat 85 to -60).
InstallerPage {
    id: page

    title: "Time zone"
    bodyHeight: 380

    property var zones: []
    readonly property real mapHeight: width * 145 / 360

    function primary() {
        page.wizard.go("hostname");
    }

    function pickAt(lon, lat) {
        let best = null;
        let bestDistance = Infinity;
        for (const zone of zones) {
            let dlon = Math.abs(zone.lon - lon);
            if (dlon > 180)
                dlon = 360 - dlon;
            const dlat = zone.lat - lat;
            const distance = dlon * dlon + dlat * dlat;
            if (distance < bestDistance) {
                bestDistance = distance;
                best = zone;
            }
        }
        if (best)
            InstallerState.timezone = best.zone;
    }

    FileView {
        path: Qt.resolvedUrl("assets/timezones.json")
        onLoaded: page.zones = JSON.parse(text())
    }

    Column {
        width: parent.width
        spacing: Theme.gap

        Item {
            id: map
            width: parent.width
            height: page.mapHeight

            Image {
                anchors.fill: parent
                source: Qt.resolvedUrl("assets/world-map.svg")
                sourceSize.width: width * 2
                fillMode: Image.Stretch
                opacity: 0.8
            }
            Repeater {
                model: page.zones
                Rectangle {
                    required property var modelData
                    readonly property bool chosen: modelData.zone === InstallerState.timezone
                    x: (modelData.lon + 180) / 360 * map.width - width / 2
                    y: (85 - modelData.lat) / 145 * map.height - height / 2
                    width: chosen ? 8 : 4
                    height: width
                    radius: width / 2
                    color: chosen ? Theme.accent : Qt.rgba(1, 1, 1, 0.6)
                }
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onPressed: mouse => page.pickAt(mouse.x / map.width * 360 - 180, 85 - mouse.y / map.height * 145)
            }
        }
        InstallerText {
            tone: "text"
            text: "Time zone: " + InstallerState.timezone.replace(/_/g, " ")
        }
        InstallerText {
            text: "Click the map to pick the nearest city."
        }
        InstallerSwitchRow {
            label: "Set the clock automatically"
            note: "Runs systemd-timesyncd in the installed system. Off disables it."
            checked: InstallerState.ntp
            onToggled: value => InstallerState.ntp = value
        }
    }
}
