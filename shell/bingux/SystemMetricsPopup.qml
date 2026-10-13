import QtQuick
import QtQuick.Layouts

ShellPopup {
    id: root
    required property var monitorWidget
    property bool customising: false
    function showPage(settings) {
        if (visible && customising === settings) {
            visible = false;
            return;
        }
        viewport.contentY = 0;
        customising = settings;
        visible = true;
    }
    readonly property real maximumHeight: Math.min(640, maximumPopupHeight)
    popupWidth: customising ? 336 : 520
    popupHeight: Math.min((customising ? content.implicitHeight : performance.implicitHeight) + contentPadding * 2, maximumHeight)
    PopupScroll {
        id: viewport
        anchors.fill: parent
        contentWidth: width
        contentHeight: root.customising ? content.implicitHeight : performance.implicitHeight
        ColumnLayout {
            id: content
            visible: root.customising
            width: parent.width
            spacing: 12
            Text {
                text: "System monitors"
                color: Theme.text
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontHeading
                font.weight: Font.DemiBold
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: root.monitorWidget.monitorNames
                    ControlRow {
                        required property string modelData
                        objectName: "monitorOption_" + modelData
                        title: root.monitorWidget.titleFor(modelData)
                        valueText: root.monitorWidget.valueFor(modelData)
                        iconName: ""
                        rowInteractive: false
                        toggleVisible: true
                        selected: root.monitorWidget.isShown(modelData)
                        toggleEnabled: (root.monitorWidget.supported(modelData) || selected) && (!selected || root.monitorWidget.selectedNames.length > 1)
                        onToggleRequested: root.monitorWidget.setShown(modelData, !selected)
                    }
                }
            }
        }
    }
    SystemPerformance {
        id: performance
        visible: !root.customising
        width: viewport.width
        height: implicitHeight
        monitorWidget: root.monitorWidget
        compact: true
    }
}
