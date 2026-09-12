import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: level
    required property var node
    required property string label
    required property string iconName
    property bool barLayout: false
    property var barWindow: null
    property var presentation: null
    property bool navigation: false
    property string navigationObjectName: ""
    property string muteObjectName: "audioLevelMute"
    property string sliderObjectName: "audioLevelVolume"
    property real maximum: 1
    property real wheelStep: 0.05
    signal devicesRequested(var trigger)
    readonly property bool available: node !== null && node.ready && node.audio !== null
    readonly property string muteIconName: available && node.audio.muted ? (label === "Microphone" ? "microphone-sensitivity-muted-symbolic" : "audio-volume-muted-symbolic") : iconName
    Layout.fillWidth: true
    implicitWidth: audioRow.implicitWidth
    implicitHeight: audioRow.implicitHeight
    RowLayout {
        id: audioRow
        anchors.fill: parent
        spacing: Theme.gap
        IconButton {
            objectName: level.muteObjectName
            barStyle: level.barLayout
            barWindow: level.barWindow
            implicitHeight: level.barLayout ? Theme.barHeight : 32
            presentation: level.presentation
            enabled: level.available
            label: (level.available && level.node.audio.muted ? "Unmute " : "Mute ") + level.label.toLowerCase()
            iconName: level.muteIconName
            Accessible.name: label
            tooltipText: label
            highlighted: level.available && level.node.audio.muted
            onClicked: level.node.audio.muted = !level.node.audio.muted
        }
        SeekSlider {
            id: gain
            warningFrom: 1
            muted: level.available && level.node.audio.muted
            objectName: level.sliderObjectName
            Layout.fillWidth: true
            implicitHeight: level.barLayout ? Theme.barHeight : Theme.sliderControlHeight
            Layout.minimumWidth: level.barLayout ? 100 : 0
            enabled: level.available
            from: 0
            to: level.maximum
            stepSize: 0.05
            snapMode: Slider.SnapAlways
            wheelEnabled: false
            value: level.available ? level.node.audio.volume : 0
            property bool wheelActive: false
            Behavior on value {
                enabled: gain.wheelActive && !gain.pressed && !Theme.reducedMotion
                NumberAnimation {
                    duration: Theme.mediaActionMotion
                    easing.type: Easing.OutCubic
                }
            }
            Timer {
                id: wheelFinish
                interval: Theme.mediaActionMotion + 60
                onTriggered: gain.wheelActive = false
            }
            function stopWheelAnimation() {
                wheelFinish.stop();
                wheelActive = false;
            }
            onPressedChanged: if (pressed)
                stopWheelAnimation()
            onEnabledChanged: if (!enabled)
                stopWheelAnimation()
            Accessible.name: level.label === "Microphone" ? "Microphone level" : "Output volume"
            onMoved: {
                stopWheelAnimation();
                level.node.audio.muted = false;
                level.node.audio.volume = value;
            }
            WheelHandler {
                id: volumeWheel
                enabled: gain.enabled && !gain.pressed
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                property real remainder: 0
                onEnabledChanged: remainder = 0
                onWheel: event => {
                    remainder += event.angleDelta.y !== 0 ? event.angleDelta.y / 120 : event.pixelDelta.y / 40;
                    const steps = Math.trunc(remainder);
                    if (steps !== 0) {
                        remainder -= steps;
                        gain.wheelActive = true;
                        level.node.audio.muted = false;
                        level.node.audio.volume = Math.max(gain.from, Math.min(gain.to, Math.round((level.node.audio.volume + steps * level.wheelStep) * 100) / 100));
                        wheelFinish.restart();
                    }
                    event.accepted = true;
                }
            }
            Connections {
                target: level
                function onNodeChanged() {
                    volumeWheel.remainder = 0;
                    gain.stopWheelAnimation();
                }
            }
        }
        Text {
            Layout.preferredWidth: 40
            horizontalAlignment: Text.AlignRight
            text: !level.available ? "—" : Math.round((gain.wheelActive ? gain.value : level.node.audio.volume) * 100) + "%"
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
            font.features: ({
                    "tnum": 1
                })
        }
        IconButton {
            objectName: level.navigationObjectName
            barStyle: level.barLayout
            barWindow: level.barWindow
            implicitHeight: level.barLayout ? Theme.barHeight : 32
            visible: level.navigation
            iconName: "go-next-symbolic"
            label: level.label === "Microphone" ? "Input devices" : "Output devices"
            onClicked: level.devicesRequested(this)
        }
    }
}
