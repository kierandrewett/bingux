import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import QtQuick.Window
import Quickshell

// A full-screen kiosk surface: the login greeter and the setup flows. The
// wallpaper is the account's blurred twin when one exists, with the sharp
// image blurred in place as the fallback. The real top bar (StatusBar) sits
// across the top and opens the real ControlCentre. A centred KioskCard holds
// the page content (set as the default children).
Item {
    id: root

    // The sharp account wallpaper; its blurred twin is the sibling file
    // named wallpaper-blur (see snapshot.rs greeter_image).
    property url wallpaper: ""
    readonly property url blurredWallpaper: wallpaper.toString() !== "" ? wallpaper.toString().replace(/[^\/]*$/, "wallpaper-blur") : ""
    property bool interactive: true
    required property var metrics
    property var hostWindow: null
    property string gnoblinCtlPath: "gnoblinctl"

    property string cardTitle: ""
    property string cardIcon: ""
    property bool backVisible: false
    property string primaryText: ""
    property bool primaryEnabled: true
    property bool primaryVisible: true
    property string secondaryText: ""
    property string secondaryIcon: ""
    property bool secondaryVisible: false
    property int cardBodyHeight: 220
    // The page content goes inside the card.
    default property alias content: card.body

    signal backRequested
    signal primaryRequested
    signal secondaryRequested

    // Tint under everything, shown while no wallpaper is available.
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.darker(Theme.accent, 1.7)
            }
            GradientStop {
                position: 1
                color: Qt.darker(Theme.accent, 3.6)
            }
        }
    }

    // Fallback: the sharp wallpaper blurred here, used only while the blurred
    // twin is missing or still loading.
    Image {
        id: sharp
        objectName: "kioskWallpaperSharp"
        anchors.fill: parent
        source: root.wallpaper
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: false
        visible: twin.status !== Image.Ready && sharp.status === Image.Ready
        layer.enabled: visible
        layer.effect: MultiEffect {
            blurEnabled: true
            blur: 1
            blurMax: 64
        }
    }

    Image {
        id: twin
        objectName: "kioskWallpaper"
        anchors.fill: parent
        source: root.blurredWallpaper
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: false
    }

    // Keeps the card and the bar readable on bright wallpapers.
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.18)
    }

    StatusBar {
        id: statusBar
        objectName: "kioskStatusBar"
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        z: 10
        chrome: false
        visible: root.interactive
        metrics: root.metrics
        hostWindow: root.hostWindow
        gnoblinCtlPath: root.gnoblinCtlPath
        onControlCentreRequested: root.toggleControlCentre()
    }

    // The real control centre, created on first use. The pendingOpen flag
    // covers the first request, which arrives before the item exists.
    property bool controlCentreWanted: false
    property bool pendingOpen: false

    function toggleControlCentre() {
        controlCentreWanted = true;
        const centre = centreLoader.item;
        if (centre)
            centre.visible = !centre.visible;
        else
            pendingOpen = true;
    }

    SystemIndicators {
        id: indicators
    }

    LazyLoader {
        id: centreLoader
        active: root.controlCentreWanted
        ControlCentre {
            objectName: "kioskControlCentre"
            indicators: indicators
            anchorWindow: root.hostWindow
            anchorItem: statusBar
            screen: root.hostWindow ? root.hostWindow.screen : null
            Component.onCompleted: if (root.pendingOpen) {
                root.pendingOpen = false;
                visible = true;
            }
            onVisibleChanged: statusBar.controlCentreOpen = visible
        }
    }

    KioskCard {
        id: card
        objectName: "kioskCard"
        anchors.centerIn: parent
        anchors.verticalCenterOffset: Math.round(root.height * 0.04)
        title: root.cardTitle
        iconName: root.cardIcon
        backVisible: root.backVisible
        primaryText: root.primaryText
        primaryEnabled: root.primaryEnabled && root.interactive
        primaryVisible: root.primaryVisible
        secondaryText: root.secondaryText
        secondaryIcon: root.secondaryIcon
        secondaryVisible: root.secondaryVisible
        bodyHeight: root.cardBodyHeight
        visible: root.interactive
        onBackRequested: root.backRequested()
        onPrimaryRequested: root.primaryRequested()
        onSecondaryRequested: root.secondaryRequested()
    }

    // Return activates the primary action unless a text field has focus,
    // where the field submits itself. Escape goes back.
    function activatePrimary(event) {
        if (!(Window.activeFocusItem instanceof TextInput) && root.primaryEnabled && root.primaryVisible) {
            root.primaryRequested();
            event.accepted = true;
        }
    }
    Keys.onReturnPressed: event => activatePrimary(event)
    Keys.onEnterPressed: event => activatePrimary(event)
    Keys.onEscapePressed: event => {
        if (root.backVisible) {
            root.backRequested();
            event.accepted = true;
        }
    }
}
