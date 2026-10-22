import QtQuick

// Wrapped body text for installer pages. Muted unless a tone is given.
// Use inside a Column or Item: the width follows the parent.
Text {
    id: root

    // "muted", "text", "danger", "warning" or "success".
    property string tone: "muted"

    width: parent.width
    wrapMode: Text.WordWrap
    color: tone === "text" ? Theme.text : tone === "danger" ? Theme.danger : tone === "warning" ? Theme.warning : tone === "success" ? Theme.success : Theme.muted
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize
}
