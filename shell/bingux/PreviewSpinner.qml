import QtQuick

Item {
    id: root
    property bool loading: false
    property color colour: Theme.muted
    implicitWidth: 28
    implicitHeight: 28
    opacity: loading ? 1 : 0
    visible: opacity > 0
    Accessible.role: Accessible.Indicator
    Accessible.name: "Loading preview"
    Behavior on opacity { NumberAnimation { duration: Theme.previewMotion } }
    Canvas {
        id: ring
        anchors.fill: parent
        onPaint: {
            const context = getContext("2d");
            context.reset();
            context.lineWidth = 2;
            context.lineCap = "round";
            context.strokeStyle = root.colour;
            context.globalAlpha = 0.2;
            context.beginPath();
            context.arc(width / 2, height / 2, width / 2 - 3, 0, Math.PI * 2);
            context.stroke();
            context.globalAlpha = 1;
            context.beginPath();
            context.arc(width / 2, height / 2, width / 2 - 3, -Math.PI / 2, Math.PI / 2);
            context.stroke();
        }
        Connections { target: root; function onColourChanged() { ring.requestPaint(); } }
        RotationAnimation on rotation {
            from: 0; to: 360; duration: 850
            loops: Animation.Infinite
            running: root.visible && root.loading && !Theme.reducedMotion
        }
    }
}
