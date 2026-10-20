import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets

// One screen for the lock client and the login greeter. The caller supplies an
// authentication object with this contract:
//   active, responseRequired, responseVisible, prompt, status, statusIsError,
//   finishing, hint, start(), respond(text), signal responseRequested()
// The visual order follows the wallpaper lock layout: date and clock at the
// top, the account identity and prompt at the bottom. Greeter-only controls
// (account list, session choice, power) slot into the two content areas.
Item {
    id: root

    required property var authentication
    // False until the host can accept credentials safely, for example until
    // the session lock is secure. Prompting starts as soon as this is true.
    property bool ready: true
    // Only one output carries the prompt; the others show the clock.
    property bool interactive: true
    property string accountName: ""
    property string accountDisplayName: ""
    property url accountImage: ""
    property string submitLabel: "Unlock"
    // Resolved by the caller: the user's own wallpaper for the lock, the
    // selected account's exported wallpaper for the greeter.
    property url wallpaper: ""
    readonly property bool twelveHourClock: /\bAP?\b|a$/i.test(Qt.locale().timeFormat(Locale.ShortFormat))
    property alias leadingContent: leading.data
    property alias trailingContent: trailing.data
    property date now: new Date()
    readonly property bool promptOpen: authentication.responseRequired && !authentication.finishing
    readonly property bool canSubmit: ready && interactive && promptOpen
    readonly property string statusText: authentication.status !== "" ? authentication.status : authentication.hint

    function submitResponse() {
        if (!canSubmit)
            return;
        authentication.respond(response.text);
        response.text = "";
    }

    function startWhenReady() {
        if (ready && interactive && !authentication.active && !authentication.finishing)
            authentication.start();
    }

    onReadyChanged: startWhenReady()

    Rectangle {
        anchors.fill: parent
        color: Theme.background
    }
    Image {
        id: backdrop
        objectName: "sessionWallpaper"
        anchors.fill: parent
        source: root.wallpaper
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        // Decoded at its natural size: a sourceSize bound to the screen
        // restarted the asynchronous load and left it in Loading.
        cache: false
    }
    // Keep the clock and the prompt readable on bright wallpapers.
    Rectangle {
        anchors.fill: parent
        visible: backdrop.status === Image.Ready
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.rgba(0, 0, 0, 0.18)
            }
            GradientStop {
                position: 0.55
                color: Qt.rgba(0, 0, 0, 0)
            }
            GradientStop {
                position: 1
                color: Qt.rgba(0, 0, 0, 0.5)
            }
        }
    }

    ColumnLayout {
        id: clock
        anchors.top: parent.top
        anchors.topMargin: Math.round(root.height * 0.09)
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 0
        Text {
            Layout.alignment: Qt.AlignHCenter
            text: Qt.formatDate(root.now, "ddd d MMM")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 24
            font.weight: Font.DemiBold
            style: Text.Normal
            opacity: 0.92
        }
        Text {
            Layout.alignment: Qt.AlignHCenter
            text: Qt.formatTime(root.now, root.twelveHourClock ? "h:mm" : "HH:mm")
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 112
            font.weight: Font.Light
            font.letterSpacing: -2
            lineHeight: 0.9
        }
    }

    ColumnLayout {
        id: identity
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Math.round(root.height * 0.07)
        anchors.horizontalCenter: parent.horizontalCenter
        width: Math.min(root.width - Theme.paddingLarge * 2, 360)
        spacing: Theme.gap

        Item {
            id: leading
            Layout.fillWidth: true
            implicitHeight: childrenRect.height
            visible: implicitHeight > 0
        }

        ClippingRectangle {
            Layout.alignment: Qt.AlignHCenter
            width: 56
            height: 56
            radius: 28
            color: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.16), Theme.background)
            Image {
                id: avatar
                anchors.fill: parent
                source: root.accountImage
                fillMode: Image.PreserveAspectCrop
                sourceSize.width: 112
                sourceSize.height: 112
                asynchronous: true
            }
            Text {
                anchors.centerIn: parent
                visible: avatar.status !== Image.Ready
                text: root.accountDisplayName.trim().charAt(0).toUpperCase()
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: 24
                font.weight: Font.DemiBold
            }
        }
        Text {
            Layout.fillWidth: true
            text: root.accountDisplayName
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            font.weight: Font.DemiBold
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
        }
        Text {
            id: status
            objectName: "sessionStatus"
            Layout.fillWidth: true
            visible: root.interactive
            text: root.statusText
            color: root.authentication.statusIsError ? Theme.danger : Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
        }

        TextField {
            id: response
            objectName: "sessionResponse"
            Layout.alignment: Qt.AlignHCenter
            Layout.preferredWidth: 220
            implicitHeight: Theme.controlHeight
            visible: root.interactive
            enabled: root.canSubmit
            opacity: root.canSubmit ? 1 : 0.6
            placeholderText: root.promptOpen && root.authentication.prompt !== "" ? root.authentication.prompt : "Password"
            echoMode: root.authentication.responseVisible ? TextInput.Normal : TextInput.Password
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            color: Theme.text
            placeholderTextColor: Theme.muted
            selectByMouse: false
            leftPadding: Theme.padding
            rightPadding: Theme.controlHeight
            Accessible.name: placeholderText
            background: Rectangle {
                radius: height / 2
                color: Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.14), Theme.background)
                border.width: 1
                border.color: response.activeFocus ? Theme.accent : Theme.surfaceColor(Qt.rgba(1, 1, 1, 0.18), Theme.background)
            }
            onAccepted: root.submitResponse()
            IconButton {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                implicitWidth: Theme.controlHeight - 8
                implicitHeight: Theme.controlHeight - 8
                enabled: root.canSubmit
                visible: response.text.length > 0
                iconName: "go-next-symbolic"
                label: root.submitLabel
                tooltipEnabled: false
                onClicked: root.submitResponse()
            }
        }
        ActionButton {
            objectName: "sessionRetry"
            Layout.alignment: Qt.AlignHCenter
            visible: root.interactive && root.ready && !root.authentication.active && !root.authentication.responseRequired && !root.authentication.finishing
            flat: true
            text: "Try again"
            onClicked: root.authentication.start()
        }

        Item {
            id: trailing
            Layout.fillWidth: true
            implicitHeight: childrenRect.height
            visible: implicitHeight > 0
        }
    }

    SystemClock {
        precision: SystemClock.Minutes
        onDateChanged: root.now = date
    }

    Component.onCompleted: {
        if (interactive)
            response.forceActiveFocus();
        startWhenReady();
    }
    Connections {
        target: root.authentication
        function onResponseRequested() {
            response.text = "";
            if (root.interactive)
                response.forceActiveFocus();
        }
    }
}
