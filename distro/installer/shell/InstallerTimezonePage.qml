import QtQuick
import Quickshell.Io

// Time zone, on a world map or from a searchable city list. The map is an
// equirectangular Natural Earth land layer (assets/world-map.svg, lat 85 to -60),
// scaled to the card, with one dot per city from zone1970.tab
// (assets/timezones.json). A click picks the nearest city within snapRadius;
// hovering shows that city's name. The list picks the same way and moves the
// marker. The clock switch sits under the list.
InstallerPage {
    id: page

    title: "Time zone"
    lead: "Click the map or search for a city to set the time zone."
    fullBody: true
    bodyHeight: 380

    readonly property real ui: InstallerState.uiScale
    property var zones: []
    property string query: ""

    readonly property real listWidth: Math.round(300 * ui)
    // The time zone line under the map: the text row and the gap above it.
    readonly property real labelHeight: Math.round(30 * ui) + Math.round(18 * ui)
    // The map takes the room beside the list, at the world map's aspect, and
    // stops at the height of the body less the line under it.
    readonly property real mapWidth: Math.max(0, Math.floor(Math.min(page.width - listWidth - Theme.gap, (page.height - labelHeight) * 360 / 145)))
    readonly property real mapHeight: mapWidth * 145 / 360
    // A click or hover within this many map pixels of a city picks or shows it.
    readonly property real snapRadius: Math.round(56 * ui)

    readonly property var chosen: {
        for (const zone of page.zones)
            if (zone.zone === InstallerState.timezone)
                return zone;
        return null;
    }
    readonly property var matches: {
        const needle = page.query.trim().toLowerCase();
        if (needle === "")
            return page.zones;
        return page.zones.filter(zone => zone.zone.replace(/_/g, " ").toLowerCase().indexOf(needle) >= 0);
    }

    function primary() {
        page.wizard.go("hostname");
    }

    function cityName(zone) {
        return zone.zone.split("/").pop().replace(/_/g, " ");
    }

    FileView {
        path: Qt.resolvedUrl("assets/timezones.json")
        onLoaded: page.zones = JSON.parse(text())
    }

    Column {
        id: mapColumn
        width: page.mapWidth
        spacing: 0

        Item {
            id: map
            width: parent.width
            height: page.mapHeight
            clip: true

            // The city under the pointer, or null.
            property var hovered: null

            function xOf(zone) {
                return (zone.lon + 180) / 360 * width;
            }
            function yOf(zone) {
                return (85 - zone.lat) / 145 * height;
            }
            // The nearest city to a map point within snapRadius, or null.
            function nearest(x, y) {
                let best = null;
                let bestDistance = page.snapRadius * page.snapRadius;
                for (const zone of page.zones) {
                    let dx = Math.abs(xOf(zone) - x);
                    if (dx > width / 2)
                        dx = width - dx;
                    const dy = yOf(zone) - y;
                    const distance = dx * dx + dy * dy;
                    if (distance < bestDistance) {
                        bestDistance = distance;
                        best = zone;
                    }
                }
                return best;
            }

            Rectangle {
                anchors.fill: parent
                radius: Theme.radius
                color: Qt.rgba(0, 0, 0, 0.3)
            }
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
                    readonly property real dot: Math.round(5 * page.ui)
                    x: map.xOf(modelData) - width / 2
                    y: map.yOf(modelData) - height / 2
                    width: dot
                    height: dot
                    radius: dot / 2
                    color: Qt.rgba(1, 1, 1, 0.55)
                }
            }
            Rectangle {
                id: ring
                visible: map.hovered !== null
                readonly property real size: Math.round(26 * page.ui)
                x: map.hovered ? map.xOf(map.hovered) - size / 2 : 0
                y: map.hovered ? map.yOf(map.hovered) - size / 2 : 0
                width: size
                height: size
                radius: size / 2
                color: "transparent"
                border.width: 2
                border.color: "#ffffff"
            }
            Rectangle {
                id: marker
                visible: page.chosen !== null
                readonly property real size: Math.round(18 * page.ui)
                x: page.chosen ? map.xOf(page.chosen) - size / 2 : 0
                y: page.chosen ? map.yOf(page.chosen) - size / 2 : 0
                width: size
                height: size
                radius: size / 2
                color: Theme.accent
                border.width: 3
                border.color: "#ffffff"
            }
            Rectangle {
                id: callout
                readonly property var target: map.hovered !== null ? map.hovered : page.chosen
                visible: target !== null
                readonly property real pointX: target ? map.xOf(target) : 0
                readonly property real pointY: target ? map.yOf(target) : 0
                width: calloutText.implicitWidth + Math.round(22 * page.ui)
                height: calloutText.implicitHeight + Math.round(12 * page.ui)
                x: Math.max(4, Math.min(map.width - width - 4, pointX - width / 2))
                y: pointY - height - 16 >= 4 ? pointY - height - 16 : pointY + 16
                radius: 8
                color: Theme.elevated
                border.width: 1
                border.color: Theme.accent
                Column {
                    id: calloutText
                    anchors.centerIn: parent
                    Text {
                        text: callout.target ? page.cityName(callout.target) : ""
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Math.round(17 * page.ui)
                        font.weight: Font.DemiBold
                    }
                    Text {
                        text: callout.target ? callout.target.zone.replace(/_/g, " ") : ""
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Math.round(14 * page.ui)
                    }
                }
            }
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onPositionChanged: mouse => map.hovered = map.nearest(mouse.x, mouse.y)
                onExited: map.hovered = null
                onClicked: mouse => {
                    const zone = map.nearest(mouse.x, mouse.y);
                    if (zone !== null)
                        InstallerState.timezone = zone.zone;
                }
            }
        }
        Item {
            width: parent.width
            height: page.labelHeight

            InstallerText {
                anchors.top: parent.top
                anchors.topMargin: Math.round(18 * page.ui)
                tone: "text"
                font.pixelSize: Math.round(24 * page.ui)
                wrapMode: Text.NoWrap
                elide: Text.ElideRight
                text: "Time zone: " + InstallerState.timezone.replace(/_/g, " ")
            }
        }
    }

    Item {
        id: listColumn
        x: page.width - width
        width: page.listWidth
        height: page.mapHeight

        InstallerField {
            id: search
            anchors.top: parent.top
            placeholderText: "Search cities or regions"
            text: page.query
            onEdited: page.query = text
        }
        ListView {
            anchors.top: search.bottom
            anchors.topMargin: Theme.gap
            anchors.bottom: clockRow.top
            anchors.bottomMargin: Theme.gap
            width: parent.width
            clip: true
            spacing: 4
            boundsBehavior: Flickable.StopAtBounds
            model: page.matches
            delegate: InstallerOption {
                required property var modelData
                width: ListView.view.width
                text: page.cityName(modelData)
                detail: modelData.zone.replace(/_/g, " ")
                selected: modelData.zone === InstallerState.timezone
                ticked: selected
                onClicked: InstallerState.timezone = modelData.zone
            }
        }
        InstallerSwitchRow {
            id: clockRow
            anchors.bottom: parent.bottom
            label: "Set the clock automatically"
            note: "Uses systemd-timesyncd."
            checked: InstallerState.ntp
            onToggled: value => InstallerState.ntp = value
        }
    }
}
