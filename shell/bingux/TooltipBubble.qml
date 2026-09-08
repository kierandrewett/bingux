import QtQuick

// Shared tooltip surface for layer windows and anchored controls.
Rectangle {
    id: root
    // Layer windows must remain mapped until the shared exit finishes.
    property bool animated: false
    property bool shown: false
    property bool presented: false
    property bool motionReady: false
    function updatePresentation() {
        if (!animated || !motionReady) return;
        motion.stop();
        if (shown) {
            Theme.beginTooltip(root);
            const duration = Theme.tooltipMotion;
            if (!presented) {
                opacity = duration > 0 ? 0 : 1;
                scale = duration > 0 ? Theme.tooltipHiddenScale : 1;
            }
            presented = true;
            fade.duration = zoom.duration = duration;
            fade.to = zoom.to = 1;
        } else {
            if (!presented) return;
            fade.duration = zoom.duration = Theme.tooltipMotion;
            fade.to = 0;
            zoom.to = Theme.tooltipHiddenScale;
        }
        motion.start();
    }
    onShownChanged: updatePresentation()
    onTextChanged: {
        if (!animated || !shown || !motionReady) return;
        motion.stop();
        opacity = 0;
        scale = Theme.tooltipHiddenScale;
        updatePresentation();
    }
    Component.onCompleted: {
        motionReady = true;
        if (animated) { opacity = 0; scale = Theme.tooltipHiddenScale; }
        updatePresentation();
    }
    Component.onDestruction: if (animated) Theme.endTooltip(root)
    ParallelAnimation {
        id: motion
        NumberAnimation { id: fade; target: root; property: "opacity"; easing.type: Easing.OutCubic }
        NumberAnimation { id: zoom; target: root; property: "scale"; easing.type: Easing.OutCubic }
        onFinished: if (!root.shown) {
            root.presented = false;
            Theme.endTooltip(root);
        }
    }
    property string text: ""
    property string supportingText: ""
    property int maximumWidth: 320
    property bool wrapText: true
    readonly property int horizontalPadding: 12
    readonly property int verticalPadding: 9
    implicitWidth: Math.min(maximumWidth,
        Math.ceil(Math.max(textMeasure.contentWidth, hintMeasure.contentWidth)) + horizontalPadding * 2)
    implicitHeight: Math.ceil(content.implicitHeight) + verticalPadding * 2
    color: Theme.popupSurface
    border.color: Theme.outline
    radius: Theme.insetRadius(Theme.cardRadius, Theme.gap)
    // Measure wrapping at a fixed limit so the bubble can fit the rendered
    // lines without creating a width binding loop with the visible labels.
    Text {
        id: textMeasure
        visible: false
        width: Math.max(0, root.maximumWidth - root.horizontalPadding * 2)
        text: root.text
        font: label.font
        textFormat: Text.PlainText
        wrapMode: label.wrapMode
    }
    Text {
        id: hintMeasure
        visible: false
        width: textMeasure.width
        text: root.supportingText
        font: hint.font
        textFormat: Text.PlainText
        wrapMode: hint.wrapMode
    }
    Column {
        id: content
        x: root.horizontalPadding
        y: root.verticalPadding
        width: Math.max(0, root.width - root.horizontalPadding * 2)
        spacing: 6
        Text {
            id: label
            width: parent.width
            text: root.text
            textFormat: Text.PlainText
            wrapMode: root.wrapText ? Text.WrapAtWordBoundaryOrAnywhere : Text.NoWrap
            elide: root.wrapText ? Text.ElideNone : Text.ElideRight
            horizontalAlignment: Text.AlignLeft
            lineHeight: 1.1
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
        }
        Text {
            id: hint
            visible: text.length > 0
            width: parent.width
            text: root.supportingText
            textFormat: Text.PlainText
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            horizontalAlignment: Text.AlignLeft
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }
}
