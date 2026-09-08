import QtQuick
import QtQuick.Layouts
import QtQuick.Controls

ShellPopup {
    id: root
    required property var monitorWidget
    property bool customising: false
    function showPage(settings) {
        if (visible && customising === settings) { visible = false; return; }
        viewport.contentY = 0;
        customising = settings;
        visible = true;
    }
    readonly property real maximumHeight: Math.max(0, Math.min(640, height * 0.65,
        anchorAbove ? anchorTop - Theme.barHeight - Theme.gap : height - Theme.dockExclusiveHeight - Theme.gap - belowAnchorY))
    popupWidth: customising ? 336 : 520
    popupHeight: !customising && performance.page !== "usage" ? maximumHeight
        : Math.min((customising ? content.implicitHeight : performance.implicitHeight) + contentPadding * 2, maximumHeight)
    contentPadding: 16
    surfaceColor: Theme.shellSurface
    Flickable {
        id: viewport
        anchors.fill: parent
        contentWidth: width
        visible: root.customising
        contentHeight: content.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { parent: root.body; x: viewport.width + 4; y: 0; height: viewport.height; width: 8 }
        ColumnLayout {
            id: content
            visible: root.customising
            width: parent.width
            spacing: 12
            Text { text: "Top bar monitors"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontHeading; font.weight: Font.DemiBold }
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
        anchors.fill: parent
        monitorWidget: root.monitorWidget
        compact: true
    }
}
