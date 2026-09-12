import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtCore
import Quickshell

ColumnLayout {
    id: root
    property url preferencesLocation: "file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-tasks.ini"
    property var previewTasks: null
    property var tasks: []
    readonly property int remaining: tasks.filter(task => !task.done).length
    spacing: Theme.gap
    Settings {
        id: saved
        location: root.preferencesLocation
        property string items: "[]"
    }
    Component.onCompleted: {
        if (previewTasks) {
            tasks = previewTasks;
            return;
        }
        try {
            const parsed = JSON.parse(saved.items);
            if (Array.isArray(parsed))
                tasks = parsed.filter(task => task && typeof task.text === "string" && typeof task.done === "boolean");
        } catch (_) {}
    }
    function focusContent() {
        input.forceActiveFocus();
    }
    function store(items) {
        if (previewTasks)
            return;
        tasks = items;
        saved.items = JSON.stringify(items);
        saved.setValue("items", saved.items);
        saved.sync();
    }
    function addTask(text) {
        if (!text.trim())
            return;
        store(tasks.concat([
            {
                text: text.trim(),
                done: false
            }
        ]));
    }
    function toggleTask(index) {
        store(tasks.map((task, i) => i === index ? {
                text: task.text,
                done: !task.done
            } : task));
    }
    function removeTask(index) {
        store(tasks.filter((_, i) => i !== index));
    }
    Item {
        Layout.fillWidth: true
        implicitHeight: 44
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            height: 1
            color: input.activeFocus ? Theme.accent : Theme.barDivider
            Behavior on color {
                ColorAnimation {
                    duration: Theme.reducedMotion ? 0 : 120
                }
            }
        }
        Text {
            x: 8
            width: 20
            anchors.verticalCenter: parent.verticalCenter
            text: "+"
            horizontalAlignment: Text.AlignHCenter
            color: input.activeFocus ? Theme.accent : Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: 22
            font.weight: Font.Light
        }
        TextField {
            id: input
            focusPolicy: root.previewTasks ? Qt.NoFocus : Qt.StrongFocus
            objectName: "taskInput"
            anchors.left: parent.left
            anchors.right: addButton.left
            anchors.leftMargin: 38
            anchors.verticalCenter: parent.verticalCenter
            implicitHeight: 40
            padding: 0
            placeholderText: "Add a task…"
            color: Theme.text
            placeholderTextColor: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSize
            selectionColor: Theme.textSelection
            selectedTextColor: Theme.text
            background: null
            onAccepted: {
                root.addTask(text);
                clear();
            }
            Accessible.name: "New task"
        }
        IconButton {
            id: addButton
            focusPolicy: root.previewTasks ? Qt.NoFocus : Qt.StrongFocus
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            iconName: "go-next-symbolic"
            label: "Add task"
            enabled: input.text.trim().length > 0
            opacity: enabled ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: Theme.reducedMotion ? 0 : 120
                }
            }
            onClicked: {
                root.addTask(input.text);
                input.clear();
                input.forceActiveFocus();
            }
        }
    }
    ListView {
        objectName: "taskList"
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: root.tasks
        spacing: 0
        ScrollBar.vertical: ScrollBar {}
        delegate: Item {
            id: taskRow
            required property var modelData
            required property int index
            width: ListView.view.width
            implicitHeight: Math.max(48, check.implicitHeight)
            HoverHandler {
                id: rowHover
            }
            Rectangle {
                anchors.fill: parent
                anchors.margins: 2
                radius: 6
                color: Theme.hover
                opacity: rowHover.hovered ? 0.3 : 0
            }
            CheckBox {
                id: check
                focusPolicy: root.previewTasks ? Qt.NoFocus : Qt.StrongFocus
                objectName: "taskCheck" + taskRow.index
                anchors.left: parent.left
                anchors.right: removeButton.left
                height: parent.height
                checked: taskRow.modelData.done
                text: taskRow.modelData.text
                onClicked: root.toggleTask(taskRow.index)
                padding: 0
                topPadding: 12
                bottomPadding: 12
                indicator: Rectangle {
                    x: 9
                    y: 14
                    width: 18
                    height: 18
                    radius: 9
                    color: check.checked ? Theme.accent : "transparent"
                    border.width: 1
                    border.color: check.visualFocus ? Theme.text : check.checked ? Theme.accent : Theme.muted
                    Text {
                        anchors.centerIn: parent
                        text: "✓"
                        visible: check.checked
                        color: Theme.background
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
                    }
                }
                contentItem: Text {
                    text: check.text
                    textFormat: Text.PlainText
                    leftPadding: 38
                    wrapMode: Text.Wrap
                    color: check.checked ? Theme.muted : Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    lineHeight: 1.2
                }
            }
            IconButton {
                id: removeButton
                focusPolicy: root.previewTasks ? Qt.NoFocus : Qt.StrongFocus
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                iconName: "window-close-symbolic"
                label: "Delete task"
                enabled: rowHover.hovered || check.activeFocus || activeFocus
                opacity: enabled ? 1 : 0
                onClicked: root.removeTask(taskRow.index)
            }
        }
        footer: Text {
            visible: root.tasks.length > 0
            width: ListView.view.width
            topPadding: 14
            leftPadding: 38
            text: root.remaining === 0 ? "All done" : root.remaining + " remaining"
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
        Text {
            visible: root.tasks.length === 0
            width: parent.width - 16
            x: 8
            y: 32
            text: "Nothing on your list yet."
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: Theme.fontSmall
        }
    }
}
