import QtQuick
import "MetricsHistory.js" as History

Item {
    id: root
    property var history: []
    property string metric: "cpu"
    property string secondaryMetric: ""
    property double endTime: Date.now()
    property int duration: 60000
    property real fixedMaximum: 100
    property color lineColor: Theme.accent
    property color secondaryColor: Theme.muted
    property bool detailed: false
    readonly property var points: History.windowPoints(history, endTime, duration)
    readonly property real maximum: fixedMaximum > 0 ? fixedMaximum : History.rateMaximum(points, [metric, secondaryMetric])
    readonly property real peak: History.peak(points, [metric])
    readonly property var inspectedPoint: detailed && pointer.containsMouse ? History.nearest(points, endTime - duration + pointer.mouseX / Math.max(1, width) * duration) : null
    readonly property var paintInputs: [points, maximum, width, height, inspectedPoint, lineColor, secondaryColor, detailed, metric, secondaryMetric, duration]
    onPaintInputsChanged: if (visible)
        graph.requestPaint()
    onVisibleChanged: if (visible)
        graph.requestPaint()
    clip: true

    Canvas {
        id: graph
        anchors.fill: parent
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const inset = root.detailed ? 3 : 1;
            const baseline = height - inset;
            const x = point => (point.at - root.endTime + root.duration) / root.duration * width;
            const y = value => baseline - Math.min(1, value / root.maximum) * (height - inset * 2);
            if (root.detailed) {
                ctx.strokeStyle = Theme.barDivider;
                ctx.lineWidth = 1;
                for (const fraction of [0, 0.5, 1]) {
                    ctx.beginPath();
                    ctx.moveTo(0, y(root.maximum * fraction));
                    ctx.lineTo(width, y(root.maximum * fraction));
                    ctx.stroke();
                }
            }
            function draw(key, colour, fill) {
                for (const points of History.segments(root.points, key)) {
                    if (fill && points.length > 1) {
                        const gradient = ctx.createLinearGradient(0, 0, 0, height);
                        gradient.addColorStop(0, Qt.rgba(colour.r, colour.g, colour.b, 0.22));
                        gradient.addColorStop(1, Qt.rgba(colour.r, colour.g, colour.b, 0.02));
                        ctx.fillStyle = gradient;
                        ctx.beginPath();
                        ctx.moveTo(x(points[0]), baseline);
                        for (const point of points)
                            ctx.lineTo(x(point), y(point[key]));
                        ctx.lineTo(x(points[points.length - 1]), baseline);
                        ctx.closePath();
                        ctx.fill();
                    }
                    ctx.strokeStyle = colour;
                    ctx.lineWidth = root.detailed ? 1.8 : 1.3;
                    ctx.lineJoin = "round";
                    ctx.lineCap = "round";
                    ctx.beginPath();
                    points.forEach((point, index) => index ? ctx.lineTo(x(point), y(point[key])) : ctx.moveTo(x(point), y(point[key])));
                    ctx.stroke();
                }
                const point = root.inspectedPoint || root.points[root.points.length - 1];
                if (point && History.valid(point[key])) {
                    ctx.fillStyle = colour;
                    ctx.beginPath();
                    ctx.arc(x(point), y(point[key]), root.detailed ? 3 : 1.7, 0, Math.PI * 2);
                    ctx.fill();
                }
            }
            draw(root.metric, root.lineColor, true);
            if (root.secondaryMetric)
                draw(root.secondaryMetric, root.secondaryColor, false);
            if (root.inspectedPoint) {
                ctx.strokeStyle = Theme.muted;
                ctx.lineWidth = 1;
                ctx.globalAlpha = 0.45;
                ctx.beginPath();
                ctx.moveTo(x(root.inspectedPoint), 0);
                ctx.lineTo(x(root.inspectedPoint), height);
                ctx.stroke();
            }
        }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        enabled: root.detailed
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
    }
    Text {
        anchors.centerIn: parent
        visible: root.detailed && root.points.length < 2
        text: "Collecting history…"
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
    }
}
