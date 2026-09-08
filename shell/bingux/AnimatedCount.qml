import QtQuick

Item {
    id: root
    property int value: 1
    property var formatter: null
    property bool stepValues: true
    property bool opticalCenter: false
    property alias font: current.font
    property color color: Theme.text
    property int displayedValue: value
    property int nextValue: value
    property real progress: 0
    property bool ready: false
    readonly property int direction: nextValue >= displayedValue ? 1 : -1
    readonly property bool animating: roll.running
    implicitWidth: Math.max(currentMetrics.width, nextMetrics.width)
    implicitHeight: Math.max(Theme.iconSize, current.implicitHeight)
    clip: true
    function label(value) { return formatter ? formatter(value) : value > 99 ? "99+" : String(value); }
    function advance() {
        if (!ready || roll.running || displayedValue === value) return;
        if (Theme.reducedMotion || label(value) === label(displayedValue)) {
            displayedValue = value; nextValue = value; progress = 0; return;
        }
        const difference = value - displayedValue;
        nextValue = !stepValues || Math.abs(difference) > 5 ? value : displayedValue + Math.sign(difference);
        progress = 0;
        roll.duration = Math.max(70, 180 / Math.max(1, Math.abs(difference)));
        roll.start();
    }
    onValueChanged: advance()
    Component.onCompleted: { displayedValue = value; nextValue = value; ready = true; }
    TextMetrics { id: currentMetrics; text: root.label(root.displayedValue); font: current.font }
    TextMetrics { id: nextMetrics; text: root.label(root.nextValue); font: current.font }
    Text {
        id: current
        anchors.horizontalCenter: parent.horizontalCenter
        y: (root.opticalCenter
            ? (root.height - currentMetrics.tightBoundingRect.height) / 2 - baselineOffset - currentMetrics.tightBoundingRect.y
            : (root.height - height) / 2) - root.direction * root.progress * root.height
        text: root.label(root.displayedValue)
        color: root.color
        opacity: 1 - root.progress
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        font.weight: Font.DemiBold
        font.features: ({"tnum": 1})
    }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        y: (root.opticalCenter
            ? (root.height - nextMetrics.tightBoundingRect.height) / 2 - baselineOffset - nextMetrics.tightBoundingRect.y
            : (root.height - height) / 2) + root.direction * (1 - root.progress) * root.height
        text: root.label(root.nextValue)
        color: root.color
        opacity: root.progress
        font: current.font
    }
    NumberAnimation {
        id: roll
        target: root; property: "progress"; from: 0; to: 1
        easing.type: Easing.OutCubic
        onFinished: {
            root.displayedValue = root.nextValue;
            root.progress = 0;
            Qt.callLater(root.advance);
        }
    }
}
