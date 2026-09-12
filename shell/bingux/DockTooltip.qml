import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root
    property string text: ""
    property real centreX: screen ? screen.width / 2 : 0
    // Distance from the screen bottom to the dock's actual top edge.
    property real anchorBottom: Theme.dockHeight + Theme.padding
    // Measure the next label independently of the currently configured window.
    // Keep a stable transparent envelope so a compositor cannot scale an old
    // buffer during a width configure. Only the bubble inside changes width.
    implicitWidth: measurement.maximumWidth
    implicitHeight: measurement.implicitHeight
    // Keep the old content until any multiline height change is configured.
    property string presentedText: ""
    function presentWhenSized() {
        if (width === implicitWidth && height === implicitHeight)
            presentedText = text;
    }
    onTextChanged: Qt.callLater(presentWhenSized)
    onWidthChanged: Qt.callLater(presentWhenSized)
    onHeightChanged: Qt.callLater(presentWhenSized)
    property bool shown: false
    visible: surface.presented
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "gnoblin-dock-tooltip"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    anchors {
        bottom: true
        left: true
    }
    margins.bottom: Math.max(Theme.gap, Math.min(anchorBottom + Theme.spaceSmall, (screen ? screen.height : 1080) - height - Theme.gap))
    margins.left: Math.max(Theme.gap, Math.min(centreX - width / 2, (screen ? screen.width : 1920) - width - Theme.gap))
    mask: Region {}
    TooltipBubble {
        id: measurement
        visible: false
        text: root.text
        width: implicitWidth
        onImplicitWidthChanged: Qt.callLater(root.presentWhenSized)
        onImplicitHeightChanged: Qt.callLater(root.presentWhenSized)
    }
    TooltipBubble {
        id: surface
        x: Math.max(0, Math.min(root.centreX - root.margins.left - width / 2, parent.width - width))
        anchors.bottom: parent.bottom
        width: implicitWidth
        height: implicitHeight
        text: root.presentedText
        animated: true
        compositorFade: PopupTransitions.fadesIn("gnoblin-dock-tooltip") && PopupTransitions.matches(Theme.tooltipMotion, Easing.OutCubic, "gnoblin-dock-tooltip")
        shown: root.shown
        transformOrigin: Item.Bottom
    }
}
