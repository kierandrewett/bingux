import QtQuick
import QtQuick.Layouts
import Quickshell
import "DesktopLayout.js" as DesktopLayout

// The top bar for every session context. The desktop shell's top bar and the
// greeter, lock and kiosk bars are the same bar: the same widget ids, the
// same zones and the same look, filtered by DesktopLayout.allowed() for the
// current SessionContext. Desktop-only slots that the default layout places
// beside shared ones (search, notifications) stay in place and are shown
// disabled; transient desktop-only items (capture, privacy, tray, overflow)
// are left out because they draw nothing at rest.
//
// The host owns the windows: it passes hostWindow for tooltips and popups,
// metrics from bingux-statusd, and handles the *Requested signals.
Item {
    id: root

    // desktop | locked | greeter | kiosk
    property string context: SessionContext.mode
    // Draws the bar surface and its bottom divider. Hosts that already paint
    // a bar behind this item turn it off.
    property bool chrome: true
    required property var metrics
    property var hostWindow: null
    property string gnoblinCtlPath: "gnoblinctl"
    property string timeoutPath: "timeout"
    // Data-only system indicators for the control centre. Always present, so the
    // control centre has state even when the controls pill is not loaded. Does not
    // draw and does not claim status monitoring; the pill's instance does.
    readonly property var indicators: dataIndicators
    SystemIndicators {
        id: dataIndicators
        visible: false
        reportsStatus: false
        timeoutPath: root.timeoutPath
    }
    // Saved desktop layout; null uses the packaged defaults.
    property var layout: DesktopEditing.desktop?.layout ?? null
    property bool metricsEnabled: true
    property bool clockSeconds: false
    property bool calendarOpen: false
    property bool controlCentreOpen: false
    property bool notificationsOpen: false
    property int notificationCount: 0

    signal searchRequested
    signal calendarRequested
    signal controlCentreRequested
    signal notificationsRequested
    signal metricsConfigureRequested
    signal metricsPerformanceRequested
    signal inputOpening

    implicitHeight: Theme.barHeight

    // Ids in the right zone that StatusBar draws, in the saved order.
    readonly property var rightOrder: {
        const zone = root.layout && Array.isArray(root.layout["top-right"]) ? root.layout["top-right"] : DesktopLayout.defaults()["top-right"];
        return zone.filter(id => ["metrics", "keyboard", "controls", "notifications"].includes(id));
    }

    function allowed(id) {
        return DesktopLayout.allowed(id, root.context);
    }
    function appearance(id, label, icon, nativeIcon, nativeText) {
        return DesktopLayout.presentation(DesktopEditing.desktop ?? {}, id, "top-right", label, icon, nativeIcon, nativeText);
    }
    function slotFor(id) {
        switch (id) {
        case "metrics":
            return metricsSlot;
        case "keyboard":
            return keyboardSlot;
        case "controls":
            return controlsSlot;
        case "notifications":
            return notificationsSlot;
        }
        return null;
    }

    QtObject {
        id: clock
        property date now: new Date()
    }
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: clock.now = new Date()
    }

    Rectangle {
        id: barSurface
        visible: root.chrome
        width: parent.width
        height: parent.height
        color: Theme.barBackground
        BackgroundEffect {
            target: barSurface
            requested: root.chrome
        }
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: Theme.barDivider
        }
    }

    BarSearchButton {
        id: searchButton
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        presentation: root.appearance("search", "Search", "system-search-symbolic", true, false)
        enabled: root.allowed("search")
        opacity: enabled ? 1 : 0.4
        onClicked: root.searchRequested()
    }

    Pill {
        id: clockPill
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        presentation: root.appearance("clock", clockLabel.text + " " + timeLabel.text, "x-office-calendar-symbolic", false, true)
        z: 1
        horizontalPadding: Theme.barPrimaryPadding
        interactive: true
        hovered: clockMouse.containsMouse
        pressed: clockMouse.pressed
        selected: root.calendarOpen
        activeFocusOnTab: true
        Accessible.name: "Calendar, " + clockLabel.text + " " + timeLabel.text
        Accessible.role: Accessible.Button
        Accessible.onPressAction: root.calendarRequested()
        Keys.onReturnPressed: root.calendarRequested()
        Keys.onSpacePressed: root.calendarRequested()
        RollingNumber {
            id: clockLabel
            text: clock.now.toLocaleDateString(Qt.locale(), "ddd d MMM")
            value: clock.now.getTime()
            pixelSize: Theme.fontSize
            fontWeight: Font.DemiBold
            shadowed: true
        }
        RollingNumber {
            id: timeLabel
            text: clock.now.toLocaleTimeString(Qt.locale(), root.clockSeconds ? "hh:mm:ss" : "hh:mm")
            value: clock.now.getTime()
            pixelSize: Theme.fontSize
            fontWeight: Font.DemiBold
            shadowed: true
        }
        MouseArea {
            id: clockMouse
            parent: clockPill
            anchors.fill: parent
            hoverEnabled: true
            onClicked: root.calendarRequested()
        }
    }

    Item {
        id: rightZone
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: Math.max(0, (root.width - clockPill.width) / 2 - Theme.gap)

        Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Theme.barControlGap
            Repeater {
                model: root.rightOrder
                Loader {
                    required property string modelData
                    visible: item ? item.visible : true
                    sourceComponent: root.slotFor(modelData)
                }
            }
        }
    }

    Component {
        id: metricsSlot
        SystemMetrics {
            systemMetrics: root.metrics
            presentation: root.appearance("metrics", "System monitors", "computer-symbolic", true, true)
            visible: root.metricsEnabled && root.allowed("metrics")
            onConfigureRequested: root.metricsConfigureRequested()
            onPerformanceRequested: root.metricsPerformanceRequested()
        }
    }

    Component {
        id: keyboardSlot
        InputSourceSelector {
            presentation: root.appearance("keyboard", displayLabel, "input-keyboard-symbolic", false, true)
            visible: root.allowed("keyboard") && (root.metrics?.desktopStateAvailable ?? false)
            parentWindow: root.hostWindow
            metrics: root.metrics
            gnoblinCtlPath: root.gnoblinCtlPath
            onOpening: root.inputOpening()
        }
    }

    Component {
        id: controlsSlot
        Pill {
            id: systemPill
            presentation: root.appearance("controls", "Control centre", "preferences-system-symbolic", true, false)
            horizontalPadding: Theme.barPrimaryPadding
            interactive: true
            hovered: systemMouse.containsMouse
            pressed: systemMouse.pressed
            selected: root.controlCentreOpen
            activeFocusOnTab: true
            Accessible.name: "Control Centre"
            Accessible.description: systemIndicators.extraStatusDescription
            Accessible.role: Accessible.Button
            Accessible.onPressAction: root.controlCentreRequested()
            Keys.onReturnPressed: root.controlCentreRequested()
            Keys.onSpacePressed: root.controlCentreRequested()
            SystemIndicators {
                id: systemIndicators
                timeoutPath: root.timeoutPath
            }
            MouseArea {
                id: systemMouse
                parent: systemPill
                anchors.fill: parent
                hoverEnabled: true
                onClicked: root.controlCentreRequested()
            }
        }
    }

    Component {
        id: notificationsSlot
        BarNotificationButton {
            presentation: root.appearance("notifications", "Notifications", "preferences-system-notifications-symbolic", true, false)
            count: root.notificationCount
            selected: root.notificationsOpen
            barWindow: root.hostWindow
            visible: root.allowed("notifications")
            onClicked: root.notificationsRequested()
        }
    }
}
