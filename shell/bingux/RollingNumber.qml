import QtQuick

Item {
    id: root
    property string text: "—"
    property real value: NaN
    property string prefix: ""
    property color color: Theme.text
    property int pixelSize: 26
    property int fontWeight: Font.Medium
    property string displayedText: text
    property string incomingText: text
    property real displayedValue: value
    property real incomingValue: value
    property real progress: 0
    property int direction: 1
    property bool ready: false
    readonly property bool animating: roll.running
    implicitWidth: prefixMeasure.advanceWidth + glyphRow.implicitWidth
    implicitHeight: Math.ceil(pixelSize * 1.3)
    clip: true
    function advance() {
        if (!ready || roll.running || displayedText === text) return;
        if (Theme.reducedMotion) {
            displayedText = text; incomingText = text;
            displayedValue = value; incomingValue = value; progress = 0;
            return;
        }
        incomingText = text;
        incomingValue = value;
        direction = Number.isFinite(value) && Number.isFinite(displayedValue) && value < displayedValue ? -1 : 1;
        progress = 0;
        roll.start();
    }
    onTextChanged: Qt.callLater(advance)
    Component.onCompleted: { displayedText = text; incomingText = text; displayedValue = value; incomingValue = value; ready = true; }
    TextMetrics { id: prefixMeasure; text: root.prefix; font: prefixText.font }
    Text {
        id: prefixText
        text: root.prefix
        anchors.verticalCenter: parent.verticalCenter
        color: root.color
        font.family: Theme.fontFamily
        font.pixelSize: root.pixelSize
        font.weight: root.fontWeight
        font.features: ({"tnum": 1})
    }
    Row {
        id: glyphRow
        x: prefixMeasure.advanceWidth
        Repeater {
            model: Math.max(root.displayedText.length, root.incomingText.length)
            Item {
                id: glyph
                objectName: "rollingGlyph" + index
                required property int index
                readonly property int slots: Math.max(root.displayedText.length, root.incomingText.length)
                readonly property string oldChar: root.displayedText[index - (slots - root.displayedText.length)] || ""
                readonly property string newChar: root.incomingText[index - (slots - root.incomingText.length)] || ""
                readonly property bool changed: oldChar !== newChar
                readonly property bool digit: /[0-9]/.test(oldChar + newChar)
                readonly property bool rolling: changed && digit
                width: Math.max(oldGlyph.advanceWidth, newGlyph.advanceWidth)
                height: root.height
                TextMetrics { id: oldGlyph; text: glyph.oldChar; font: prefixText.font }
                TextMetrics { id: newGlyph; text: glyph.newChar; font: prefixText.font }
                Text {
                    objectName: "oldRollingDigit"
                    text: glyph.oldChar
                    font: prefixText.font
                    color: root.color
                    y: (glyph.height - height) / 2 - (glyph.rolling ? root.direction * root.progress * glyph.height : 0)
                    opacity: glyph.changed ? 1 - root.progress : 1
                }
                Text {
                    objectName: "newRollingDigit"
                    visible: glyph.changed
                    text: glyph.newChar
                    font: prefixText.font
                    color: root.color
                    y: (glyph.height - height) / 2 + (glyph.rolling ? root.direction * (1 - root.progress) * glyph.height : 0)
                    opacity: root.progress
                }
            }
        }
    }
    NumberAnimation {
        id: roll
        target: root; property: "progress"; from: 0; to: 1
        duration: 140
        easing.type: Easing.OutCubic
        onFinished: {
            root.displayedText = root.incomingText;
            root.displayedValue = root.incomingValue;
            root.progress = 0;
            Qt.callLater(root.advance);
        }
    }
}
