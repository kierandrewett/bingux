import QtQuick
import QtQuick.Layouts
import Quickshell

// Shared tooltip surface for layer windows and anchored controls.
Rectangle {
    id: root
    BackgroundEffect {
        id: backgroundBlur
        target: root
        radius: Theme.tooltipRadius
    }
    readonly property bool backgroundBlurRequested: backgroundBlur.requested
    SurfaceFade {
        target: root
    }
    // Layer windows must remain mapped until the shared exit finishes.
    property bool animated: false
    property bool shown: false
    property bool presented: false
    property bool motionReady: false
    property bool compositorFade: false
    function updatePresentation() {
        if (!animated || !motionReady)
            return;
        motion.stop();
        if (compositorFade) {
            opacity = 1;
            if (shown) {
                Theme.beginTooltip(root);
                if (!presented)
                    scale = Theme.tooltipHiddenScale;
                presented = true;
                fade.to = 1;
                zoom.to = 1;
                fade.duration = zoom.duration = Theme.tooltipMotion;
                motion.start();
            } else {
                presented = false;
                Theme.endTooltip(root);
            }
            return;
        }
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
            if (!presented)
                return;
            fade.duration = zoom.duration = Theme.tooltipMotion;
            fade.to = 0;
            zoom.to = Theme.tooltipHiddenScale;
        }
        motion.start();
    }
    onShownChanged: updatePresentation()
    onTextChanged: {
        if (!animated || !shown || !motionReady)
            return;
        motion.stop();
        opacity = compositorFade ? 1 : 0;
        scale = Theme.tooltipHiddenScale;
        updatePresentation();
    }
    Component.onCompleted: {
        motionReady = true;
        if (animated) {
            opacity = 0;
            scale = Theme.tooltipHiddenScale;
        }
        updatePresentation();
    }
    Component.onDestruction: if (animated)
        Theme.endTooltip(root)
    ParallelAnimation {
        id: motion
        NumberAnimation {
            id: fade
            target: root
            property: "opacity"
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            id: zoom
            target: root
            property: "scale"
            easing.type: Easing.OutCubic
        }
        onFinished: if (!root.shown) {
            root.presented = false;
            Theme.endTooltip(root);
        }
    }
    property string text: ""
    property string supportingText: ""
    property var details: []
    property int maximumWidth: 320
    property bool wrapText: true
    readonly property int horizontalPadding: Theme.tooltipHorizontalPadding
    readonly property int verticalPadding: Theme.tooltipVerticalPadding
    implicitWidth: Math.min(maximumWidth, Math.ceil(Math.max(textMeasure.contentWidth, hintMeasure.contentWidth, detailMeasure.implicitWidth)) + horizontalPadding * 2)
    implicitHeight: Math.ceil(content.implicitHeight) + verticalPadding * 2
    color: Theme.tooltipSurface
    border.width: 1
    border.color: Theme.tooltipOutline
    radius: Theme.tooltipRadius
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
        id: detailMeasure
        visible: false
        Repeater {
            model: root.details
            RowLayout {
                required property var modelData
                spacing: Theme.tooltipRowSpacing
                Image {
                    readonly property string iconName: modelData.icon || modelData.appIcon || ""
                    visible: iconName !== ""
                    Layout.preferredWidth: visible ? Theme.tooltipIconSize : 0
                    Layout.preferredHeight: visible ? Theme.tooltipIconSize : 0
                    source: visible ? Quickshell.iconPath(iconName, "application-x-executable") : ""
                    sourceSize: Qt.size(Theme.tooltipIconSize, Theme.tooltipIconSize)
                    fillMode: Image.PreserveAspectFit
                }
                ColumnLayout {
                    Text {
                        text: modelData.label || modelData.app || ""
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize
                    }
                    Text {
                        text: modelData.value || modelData.device || ""
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                }
            }
        }
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
        Repeater {
            model: root.details
            RowLayout {
                required property var modelData
                width: content.width
                spacing: Theme.tooltipRowSpacing
                Image {
                    readonly property string iconName: modelData.icon || modelData.appIcon || ""
                    visible: iconName !== ""
                    Layout.preferredWidth: visible ? Theme.tooltipIconSize : 0
                    Layout.preferredHeight: visible ? Theme.tooltipIconSize : 0
                    source: visible ? Quickshell.iconPath(iconName, "application-x-executable") : ""
                    sourceSize: Qt.size(Theme.tooltipIconSize, Theme.tooltipIconSize)
                    fillMode: Image.PreserveAspectFit
                    Layout.alignment: Qt.AlignTop
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 2
                    Text {
                        text: modelData.label || modelData.app || ""
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize
                        font.weight: Font.DemiBold
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                    Text {
                        text: (modelData.value || modelData.device || "") + (modelData.muted ? " (muted)" : "")
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
            }
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
