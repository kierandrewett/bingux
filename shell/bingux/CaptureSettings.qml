import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtCore

Item {
    id: root
    objectName: "captureSettingsPage"
    required property var preferences
    property bool audioAvailable: false
    property bool advanced: false
    readonly property bool video: preferences.kind === "recording"
    readonly property bool systemAudio: preferences.audio === "system" || preferences.audio === "both"
    readonly property bool microphone: preferences.audio === "microphone" || preferences.audio === "both"
    readonly property bool pickingFolder: folderPicker.visible
    readonly property alias folderDialog: folderPicker
    readonly property url defaultFolder: StandardPaths.writableLocation(video ? StandardPaths.MoviesLocation : StandardPaths.PicturesLocation)
    implicitHeight: content.implicitHeight + Theme.popupPadding * 2
    signal back
    function setAudio(system, microphone) {
        preferences.audio = system && microphone ? "both" : system ? "system" : microphone ? "microphone" : "none";
    }
    FolderDialog {
        id: folderPicker
        objectName: "captureFolderPicker"
        title: "Choose a capture folder"
        currentFolder: root.preferences.directory ? "file://" + encodeURIComponent(root.preferences.directory).replace(/%2F/g, "/") : root.defaultFolder
        onAccepted: root.preferences.directory = decodeURIComponent(selectedFolder.toString().replace(/^file:\/\//, ""))
    }

    Flickable {
        anchors.fill: parent
        anchors.margins: Theme.popupPadding
        contentHeight: content.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}
        ColumnLayout {
            id: content
            width: parent.width
            spacing: Theme.padding
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.gap
                ActionButton {
                    objectName: "captureSettingsBack"
                    Layout.preferredWidth: 36
                    Layout.preferredHeight: 36
                    Layout.minimumWidth: 36
                    Layout.maximumWidth: 36
                    Layout.minimumHeight: 36
                    Layout.maximumHeight: 36
                    implicitWidth: 36
                    implicitHeight: 36
                    Accessible.name: "Back to capture"
                    flat: true
                    onClicked: root.back()
                    contentItem: Item {
                        SymbolicIcon {
                            anchors.centerIn: parent
                            implicitSize: 20
                            source: Qt.resolvedUrl("icons/capture-back.svg")
                        }
                    }
                }
                Text {
                    Layout.fillWidth: true
                    text: root.video ? "Recording settings" : "Screenshot settings"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    font.weight: Font.DemiBold
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                SettingsRow {
                    objectName: "captureSystemAudio"
                    visible: root.video
                    title: "System audio"
                    subtitle: "Sound from apps and games"
                    implicitHeight: 56
                    rowInteractive: false
                    toggleVisible: true
                    toggleEnabled: root.audioAvailable
                    toggleChecked: root.systemAudio
                    onToggleRequested: root.setAudio(!root.systemAudio, root.microphone)
                }
                SettingsRow {
                    objectName: "captureMicrophoneAudio"
                    visible: root.video
                    title: "Microphone"
                    subtitle: "Default input device"
                    implicitHeight: 56
                    rowInteractive: false
                    toggleVisible: true
                    toggleEnabled: root.audioAvailable
                    toggleChecked: root.microphone
                    onToggleRequested: root.setAudio(root.systemAudio, !root.microphone)
                }
                SettingsRow {
                    title: "Show pointer"
                    rowInteractive: false
                    toggleVisible: true
                    toggleChecked: preferences.cursor
                    onToggleRequested: preferences.cursor = !preferences.cursor
                }
                SettingsRow {
                    visible: !root.video
                    title: "Copy to clipboard"
                    rowInteractive: false
                    toggleVisible: true
                    toggleChecked: preferences.copy
                    onToggleRequested: preferences.copy = !preferences.copy
                }
            }
            Text {
                visible: root.video && !root.audioAvailable
                Layout.fillWidth: true
                text: "Audio recording is unavailable on this system."
                wrapMode: Text.WordWrap
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            Choice {
                label: "Start delay"
                choices: ["Off", "3 s", "5 s", "10 s"]
                values: [0, 3, 5, 10]
                value: preferences.delay
                onChosen: value => preferences.delay = value
            }
            Choice {
                visible: !root.video
                label: "Image format"
                choices: ["PNG", "JPEG"]
                values: ["png", "jpeg"]
                value: preferences.format
                onChosen: value => preferences.format = value
            }
            Choice {
                visible: root.video || preferences.format === "jpeg"
                label: "Quality"
                choices: ["Compact", "Balanced", "High"]
                values: ["compact", "balanced", "high"]
                value: preferences.quality
                onChosen: value => preferences.quality = value
            }
            SettingsRow {
                objectName: "captureSaveFolder"
                title: "Save to"
                subtitle: preferences.directory || (root.video ? "Videos / Recordings" : "Pictures / Screenshots")
                navigation: true
                actionLabel: preferences.directory ? "Reset" : ""
                onActionTriggered: preferences.directory = ""
                onClicked: folderPicker.open()
            }
            SettingsRow {
                objectName: "captureAdvancedSettings"
                title: "Advanced"
                navigation: true
                navigationRotation: root.advanced ? 90 : 0
                onClicked: root.advanced = !root.advanced
            }
            ColumnLayout {
                visible: root.advanced
                Layout.fillWidth: true
                spacing: Theme.padding
                Choice {
                    visible: root.video
                    label: "Frame rate"
                    choices: ["15 fps", "30 fps", "60 fps"]
                    values: [15, 30, 60]
                    value: preferences.fps
                    onChosen: value => preferences.fps = value
                }
                Choice {
                    visible: root.video
                    label: "Resolution limit"
                    choices: ["720p", "1080p", "1440p", "4K", "Native"]
                    values: [720, 1080, 1440, 2160, 0]
                    value: preferences.maxHeight
                    onChosen: value => preferences.maxHeight = value
                }
                Choice {
                    visible: root.video
                    label: "Encoding"
                    choices: ["Automatic", "Software"]
                    values: ["auto", "cpu"]
                    value: preferences.encoder
                    onChosen: value => preferences.encoder = value
                }
                Choice {
                    label: "Capture backend"
                    choices: ["Automatic", "Desktop portal"]
                    values: ["auto", "portal"]
                    value: preferences.backend
                    onChosen: value => preferences.backend = value
                }
            }
        }
    }

    component Choice: ColumnLayout {
        id: choice
        required property string label
        required property var choices
        required property var values
        required property var value
        signal chosen(var value)
        Layout.fillWidth: true
        spacing: Theme.gap
        Text {
            text: choice.label
            Layout.leftMargin: Theme.padding
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        SegmentedControl {
            objectName: "captureChoice" + choice.label
            objectNamePrefix: "captureChoice" + choice.label
            Layout.fillWidth: true
            implicitHeight: 36
            options: choice.choices
            currentValue: choice.choices[Math.max(0, choice.values.indexOf(choice.value))]
            accessiblePrefix: choice.label + ": "
            onSelected: label => choice.chosen(choice.values[choice.choices.indexOf(label)])
        }
    }
}
