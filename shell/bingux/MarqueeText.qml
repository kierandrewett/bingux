import QtQuick
import QtQuick.Effects

Item {
    id: root
    property string text: ""
    property string restText: text
    property int textFormat: Text.StyledText
    property bool active: false
    property color color: Theme.text
    property int pixelSize: Theme.fontSize
    property int fontWeight: Font.Normal
    property real offset: 0
    property bool revealed: false
    readonly property real textWidth: measure.implicitWidth
    readonly property real overflow: Math.max(0, textWidth - width)
    readonly property real travel: textWidth + 32
    property var layoutInputs: [text, restText, width, active, Theme.reducedMotion, pixelSize, fontWeight]
    onLayoutInputsChanged: reset()
    implicitHeight: measure.implicitHeight
    clip: true

    function reset() {
        dwell.stop();
        scroll.stop();
        offset = 0;
        revealed = false;
        if (active && !Theme.reducedMotion) dwell.restart();
    }
    function scrollBy(delta) {
        dwell.stop();
        scroll.stop();
        revealed = true;
        offset = Math.max(0, Math.min(overflow, offset + delta));
    }
    Timer {
        id: dwell
        interval: 500
        onTriggered: {
            if (root.active && root.overflow > 0) {
                root.revealed = true;
                scroll.restart();
            }
        }
    }
    SequentialAnimation {
        id: scroll
        loops: Animation.Infinite
        NumberAnimation {
            target: root; property: "offset"
            from: 0; to: root.travel
            duration: Math.max(1000, root.travel / 32 * 1000)
            easing.type: Easing.Linear
        }
        ScriptAction { script: root.offset = 0 }
        PauseAnimation { duration: 800 }
    }
    Text {
        id: measure
        visible: false
        text: root.text
        textFormat: root.textFormat
        font.family: Theme.fontFamily
        font.pixelSize: root.pixelSize
        font.weight: root.fontWeight
    }
    Rectangle {
        id: edgeMask
        anchors.fill: parent
        visible: false
        layer.enabled: root.revealed && root.overflow > 0
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: root.offset > 0 ? "transparent" : "white" }
            GradientStop { position: Math.min(0.49, Theme.padding / Math.max(1, root.width)); color: "white" }
            GradientStop { position: 1 - Math.min(0.49, Theme.padding / Math.max(1, root.width)); color: "white" }
            GradientStop { position: 1; color: "transparent" }
        }
    }
    Item {
        anchors.fill: parent
        layer.enabled: root.revealed && root.overflow > 0
        layer.effect: MultiEffect { maskEnabled: true; maskSource: edgeMask; maskThresholdMin: 0.5; maskSpreadAtMin: 1.0 }
        Text {
            x: -root.offset
            width: root.revealed ? Math.max(root.width, root.textWidth) : root.width
            text: root.revealed ? root.text : root.restText
            textFormat: root.textFormat
            elide: root.revealed ? Text.ElideNone : Text.ElideRight
            color: root.color
            font: measure.font
        }
        Text {
            visible: scroll.running
            x: root.travel - root.offset
            text: root.text
            textFormat: root.textFormat
            color: root.color
            font: measure.font
        }
    }
}
