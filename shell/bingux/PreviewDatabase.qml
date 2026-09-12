import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

Rectangle {
    id: root
    objectName: "previewDatabase"
    property string path: ""
    property var helperCommand: []
    property var initialData: null
    property var tableData: initialData
    property real zoom: 1
    property string selectedTable: initialData ? initialData.table : ""
    property int offset: 0
    property bool requested: false
    property bool loading: false
    property string error: ""
    color: Theme.searchSurface
    readonly property real columnWidth: Math.max(140, horizontal.width / Math.max(1, tableData ? tableData.columns.length : 1)) * zoom
    onSelectedTableChanged: tableChoice.currentIndex = initialData ? initialData.tables.indexOf(selectedTable) : -1
    function load(table, offset) {
        root.selectedTable = table;
        root.offset = offset;
        root.requested = true;
        root.loading = true;
    }
    DocumentPreviewRequest {
        active: root.requested
        command: root.helperCommand.concat(["table", root.path, root.selectedTable, String(root.offset)])
        onResponse: response => {
            root.loading = false;
            root.error = response.error || "";
            if (!response.error)
                root.tableData = response;
        }
    }
    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6
        ComboBox {
            id: tableChoice
            Layout.fillWidth: true
            implicitHeight: 30
            model: root.initialData ? root.initialData.tables : []
            onActivated: root.load(currentText, 0)
            Accessible.name: "Database table"
            background: Rectangle {
                radius: 6
                color: Theme.elevated
                border.width: 1
                border.color: Theme.outline
            }
            contentItem: Text {
                text: tableChoice.displayText
                leftPadding: 8
                rightPadding: 28
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            indicator: SymbolicIcon {
                x: tableChoice.width - width - 8
                y: (tableChoice.height - height) / 2
                implicitSize: 14
                source: Quickshell.iconPath("pan-down-symbolic")
                color: Theme.muted
            }
        }
        Flickable {
            id: horizontal
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: Math.max(width, root.tableData ? root.tableData.columns.length * root.columnWidth : 0)
            contentHeight: height
            boundsBehavior: Flickable.StopAtBounds
            HorizontalWheelScroll {
                viewport: horizontal
            }
            ScrollBar.horizontal: ScrollBar {}
            ListView {
                anchors.top: parent.top
                width: horizontal.contentWidth
                height: horizontal.height - 10
                clip: true
                model: root.tableData ? [root.tableData.columns].concat(root.tableData.rows) : []
                opacity: root.loading ? 0.35 : 1
                Behavior on opacity {
                    NumberAnimation {
                        duration: Theme.previewMotion
                    }
                }
                ScrollBar.vertical: ScrollBar {}
                delegate: Row {
                    required property var modelData
                    required property int index
                    property bool heading: index === 0
                    height: 27 * root.zoom
                    Repeater {
                        model: modelData
                        Rectangle {
                            required property var modelData
                            width: root.columnWidth
                            height: parent.height
                            color: parent.heading ? Theme.elevated : "transparent"
                            border.width: 1
                            border.color: Theme.outline
                            Text {
                                anchors.fill: parent
                                anchors.margins: 5
                                text: String(modelData)
                                textFormat: Text.PlainText
                                color: Theme.text
                                font.pixelSize: 11 * root.zoom
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
            PreviewSpinner {
                anchors.centerIn: parent
                loading: root.loading
            }
            Text {
                anchors.centerIn: parent
                visible: root.error !== "" || (root.tableData && root.tableData.tables.length === 0)
                text: root.error || "No tables in this database"
                color: Theme.muted
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                text: root.tableData && root.tableData.rows.length ? "Rows " + (root.tableData.offset + 1) + "–" + (root.tableData.offset + root.tableData.rows.length) : "No rows"
                color: Theme.muted
                font.pixelSize: 11
            }
            ActionButton {
                text: "Previous"
                flat: true
                implicitHeight: 28
                enabled: !root.loading && root.offset > 0
                onClicked: root.load(root.selectedTable, root.offset - 100)
            }
            ActionButton {
                text: "Next"
                flat: true
                implicitHeight: 28
                enabled: !root.loading && root.tableData && root.tableData.hasMore
                onClicked: root.load(root.selectedTable, root.offset + 100)
            }
        }
    }
}
