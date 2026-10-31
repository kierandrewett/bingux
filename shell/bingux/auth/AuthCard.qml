import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import ".."

// The account identity and the credential form, in one column: a leading
// slot, the picture, the name, a status line, a content slot (the startup
// progress takes it) and the frosted response field with a retry link.
//
// This item holds no authentication logic. The host sets what to show and
// reacts to `submitted(text)`; the text is handed over once and the field is
// cleared at the same moment, so a secret never stays in the item.
ColumnLayout {
    id: root

    property AuthTheme theme: AuthTheme {}
    // The wallpaper layer that the field samples for its frosted look.
    property Item backdrop: null
    property string displayName: ""
    property url avatarSource: ""
    property bool showAccount: true
    property bool interactive: true
    // Hide the field and the retry link, for example while a launch shows its progress.
    property bool showForm: true
    property string statusText: ""
    property bool statusIsError: false
    property string promptText: ""
    property bool echoVisible: false
    property bool canSubmit: false
    property bool busy: false
    // False freezes the activity ring: a hidden greeter must not keep the GPU
    // or CPU busy drawing it.
    property bool spinning: true
    property string submitLabel: "Unlock"
    property bool showRetry: false
    property string retryText: "Try again"
    property alias leadingContent: leading.data
    property alias content: extra.data
    readonly property alias field: response

    signal submitted(string text)
    signal retryRequested

    function clear() {
        response.text = "";
    }

    function focusField() {
        if (interactive)
            response.forceActiveFocus();
    }

    // A rejected attempt shakes the field: the sign that it failed without
    // reading the status line.
    function reject() {
        shake.restart();
    }

    function submit() {
        if (!canSubmit)
            return;
        const text = response.text;
        response.text = "";
        submitted(text);
    }

    spacing: Theme.gap

    Item {
        id: leading
        Layout.fillWidth: true
        implicitHeight: childrenRect.height
        visible: implicitHeight > 0
    }

    SessionShadow {
        Layout.alignment: Qt.AlignHCenter
        visible: root.showAccount
        padding: 32
        blur: 0.9
        offsetY: 6
        colour: Qt.rgba(0, 0, 0, 0.35)
        UserAvatar {
            size: 72
            source: root.avatarSource
            name: root.displayName
        }
    }
    SessionShadow {
        Layout.fillWidth: true
        visible: root.showAccount
        implicitHeight: accountLabel.implicitHeight
        padding: 16
        blur: 0.5
        offsetY: 1
        colour: Qt.rgba(0, 0, 0, 0.45)
        Text {
            id: accountLabel
            textFormat: Text.PlainText
            width: parent.width
            text: root.displayName
            color: root.theme.ink
            font.family: Theme.fontFamily
            font.pixelSize: 16
            font.weight: Font.DemiBold
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
    }
    // Activity while bsm checks the credentials or finishes the sign-in. A ring
    // in the screen's ink; it holds still under reduced motion.
    Item {
        id: spinner
        Layout.alignment: Qt.AlignHCenter
        implicitWidth: 20
        implicitHeight: 20
        visible: root.busy
        Accessible.role: Accessible.Indicator
        Accessible.name: "Working"
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                strokeColor: root.theme.ink
                strokeWidth: 2
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                PathAngleArc {
                    centerX: 10
                    centerY: 10
                    radiusX: 8
                    radiusY: 8
                    startAngle: 0
                    sweepAngle: 260
                }
            }
            RotationAnimator on rotation {
                from: 0
                to: 360
                duration: 900
                loops: Animation.Infinite
                running: spinner.visible && root.spinning && !root.theme.reduceMotion
            }
        }
    }
    Text {
        id: status
        textFormat: Text.PlainText
        objectName: "sessionStatus"
        Layout.fillWidth: true
        Layout.topMargin: -2
        visible: root.interactive
        text: root.statusText
        color: root.statusIsError ? root.theme.danger : root.theme.inkMuted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        Accessible.role: Accessible.StaticText
        Accessible.name: text
        // A new message pulses in rather than snapping.
        onTextChanged: statusPulse.restart()
        SequentialAnimation {
            id: statusPulse
            NumberAnimation {
                target: status
                property: "opacity"
                to: 0.4
                duration: root.theme.motion
            }
            NumberAnimation {
                target: status
                property: "opacity"
                to: 1
                duration: root.theme.motion
            }
        }
    }

    Item {
        id: extra
        Layout.fillWidth: true
        implicitHeight: childrenRect.height
        visible: implicitHeight > 0
    }

    TextField {
        id: response
        objectName: "sessionResponse"
        Layout.alignment: Qt.AlignHCenter
        Layout.topMargin: Theme.spaceSmall
        Layout.preferredWidth: 200
        implicitHeight: 32
        visible: root.interactive && root.showForm
        enabled: root.canSubmit
        opacity: root.canSubmit ? 1 : 0.55
        // The prompt comes from PAM. A TextField placeholder auto-detects rich
        // text, so the prompt is drawn by a plain-text item below instead.
        placeholderText: ""
        echoMode: root.echoVisible ? TextInput.Normal : TextInput.Password
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall + 1
        color: root.theme.ink
        placeholderTextColor: root.theme.inkMuted
        selectByMouse: false
        leftPadding: Theme.padding
        rightPadding: 32
        // A secret field never offers completions or keeps predictions.
        inputMethodHints: root.echoVisible ? Qt.ImhNone : Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
        Accessible.name: root.promptText
        background: SessionGlass {
            backdrop: root.backdrop
            tint: root.theme.glass
            outline: response.activeFocus ? root.theme.glassOutlineFocus : root.theme.glassOutline
            Behavior on outline {
                ColorAnimation {
                    duration: root.theme.motion
                }
            }
        }
        onAccepted: root.submit()
        transform: Translate {
            id: shakeOffset
        }
        SequentialAnimation {
            id: shake
            loops: 1
            NumberAnimation {
                target: shakeOffset
                property: "x"
                to: -8
                duration: root.theme.motion / 2
            }
            NumberAnimation {
                target: shakeOffset
                property: "x"
                to: 8
                duration: root.theme.motion / 2
            }
            NumberAnimation {
                target: shakeOffset
                property: "x"
                to: -4
                duration: root.theme.motion / 2
            }
            NumberAnimation {
                target: shakeOffset
                property: "x"
                to: 0
                duration: root.theme.motion / 2
            }
        }
        Text {
            objectName: "sessionPrompt"
            textFormat: Text.PlainText
            anchors.left: parent.left
            anchors.leftMargin: response.leftPadding
            anchors.right: parent.right
            anchors.rightMargin: response.rightPadding
            anchors.verticalCenter: parent.verticalCenter
            visible: response.text.length === 0 && !response.preeditText
            text: root.promptText
            color: root.theme.inkMuted
            font: response.font
            elide: Text.ElideRight
        }
        AbstractButton {
            id: submitButton
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            width: 24
            height: 24
            enabled: root.canSubmit
            visible: response.text.length > 0
            Accessible.name: root.submitLabel
            onClicked: root.submit()
            background: Rectangle {
                radius: 12
                color: submitButton.down ? Qt.rgba(1, 1, 1, 0.45) : Qt.rgba(1, 1, 1, 0.3)
            }
            contentItem: SymbolicIcon {
                implicitSize: 12
                source: BinguxIcons.source("go-next-symbolic")
                color: root.theme.ink
            }
        }
    }
    SessionLink {
        objectName: "sessionRetry"
        Layout.alignment: Qt.AlignHCenter
        visible: root.interactive && root.showForm && root.showRetry
        text: root.retryText
        onClicked: root.retryRequested()
    }
}
