import QtQuick
import QtQuick.Layouts
import "DesktopLayout.js" as DesktopLayout

Item {
    id: root
    required property var systemMetrics
    required property var privacyState
    property var barWindow: null
    function appearance(label, icon) {
        return DesktopLayout.presentation(DesktopEditing.desktop, "privacy", DesktopLayout.placement(DesktopEditing.desktop, "privacy"), label, icon, true, false);
    }
    readonly property bool screenSharing: privacyState.screenSharing || (!privacyState.available && systemMetrics.screenSharing)
    readonly property bool active: sharingVisible || privacyState.cameraInUse || privacyState.microphoneInUse || systemMetrics.locationInUse
    visible: active
    property bool sharingVisible: screenSharing
    property double sharingShownAt: Date.now()
    readonly property bool panelLayout: ["sidebar", "control-centre"].includes(DesktopLayout.placement(DesktopEditing.desktop, "privacy"))
    readonly property var shownIndicators: privacyRow.children.filter(item => item.visible)
    readonly property real naturalWidth: shownIndicators.reduce((sum, item) => sum + item.implicitWidth, 0) + Math.max(0, shownIndicators.length - 1) * Theme.barControlGap
    readonly property real availableWidth: panelLayout && parent ? parent.width : naturalWidth
    implicitWidth: naturalWidth
    Layout.fillWidth: panelLayout
    Layout.maximumWidth: availableWidth
    implicitHeight: Math.max(Theme.barHeight, privacyRow.implicitHeight)
    width: Math.min(implicitWidth, availableWidth)
    height: implicitHeight
    onScreenSharingChanged: {
        sharingDelay.stop();
        if (screenSharing) {
            if (!sharingVisible)
                sharingShownAt = Date.now();
            sharingVisible = true;
        } else {
            sharingDelay.interval = Math.max(0, 5000 - (Date.now() - sharingShownAt));
            sharingDelay.start();
        }
    }
    Timer {
        id: sharingDelay
        onTriggered: root.sharingVisible = false
    }
    component Indicator: ActivityIndicator {
        width: Math.min(implicitWidth, privacyRow.width)
        height: implicitHeight
    }
    Flow {
        id: privacyRow
        width: root.width
        spacing: Theme.barControlGap
        Indicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "screenSharingIndicator"
            visible: root.sharingVisible
            barWindow: root.barWindow
            filled: true
            iconName: "screen-shared-symbolic"
            trailingIcon: "screencast-stop-symbolic"
            interactive: root.screenSharing && root.privacyState.available
            tooltip: root.screenSharing ? "Stop screen sharing" : "Screen sharing ended"
            onClicked: root.privacyState.stopSharing()
        }
        Indicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "cameraIndicator"
            visible: root.privacyState.cameraInUse
            barWindow: root.barWindow
            iconName: "camera-web-symbolic"
            tooltip: "Camera in use"
        }
        Indicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "microphoneIndicator"
            visible: root.privacyState.microphoneInUse
            barWindow: root.barWindow
            iconName: "microphone-sensitivity-high-symbolic"
            tooltip: root.privacyState.microphoneTooltip
        }
        Indicator {
            presentation: root.appearance(tooltip, iconName)
            objectName: "locationIndicator"
            visible: root.systemMetrics.locationInUse
            barWindow: root.barWindow
            iconName: "find-location-symbolic"
            tooltip: "Location in use"
        }
    }
}
