import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "DesktopLayout.js" as DesktopLayout

pragma ComponentBehavior: Bound

Item {
    id: root
    property bool preview: false
    property bool barStyle: true
    property var barWindow: null
    property bool enabledForInteraction: true
    readonly property string widgetId: "workspaces"
    readonly property var sampleWorkspaces: [
        { id: "code", number: 1, name: "Code", active: true, windows: 2 },
        { id: "web", number: 2, name: "Web", active: false, windows: 1 },
        { id: "mail", number: 3, name: "Mail", active: false, windows: 0 }
    ]
    readonly property var workspaces: preview ? sampleWorkspaces : WorkspaceState.workspaces
    readonly property var activeWorkspace: workspaces.find(workspace => workspace.active) || null
    readonly property string buttonLabel: activeWorkspace ? activeWorkspace.number + " · " + activeWorkspace.name :
        preview ? "1 · Code" : WorkspaceState.errorMessage ? "Workspaces unavailable" :
        WorkspaceState.connectionState === "connecting" || WorkspaceState.requestState === "workspace-list" ? "Loading workspaces…" :
        WorkspaceState.connectionState === "ready" ? "No workspaces" : "Workspaces unavailable"
    readonly property var presentation: Object.assign(DesktopLayout.presentation(
        DesktopEditing.desktop,
        widgetId,
        DesktopLayout.placement(DesktopEditing.desktop, widgetId) || "top-right",
        buttonLabel,
        "view-grid-symbolic",
        true,
        true
    ), { custom: true })
    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight
    Accessible.role: Accessible.Grouping
    Accessible.name: buttonLabel

    IconButton {
        id: button
        anchors.fill: parent
        iconName: "view-grid-symbolic"
        label: root.buttonLabel
        tooltipText: root.activeWorkspace ? "Workspace " + root.activeWorkspace.number + ": " + root.activeWorkspace.name : root.buttonLabel
        barStyle: root.barStyle
        barWindow: root.barWindow
        presentation: root.presentation
        highlighted: popup.visible
        enabled: root.preview || (root.enabledForInteraction && WorkspaceState.available && !DesktopEditing.active)
        onClicked: popup.visible = !popup.visible
    }

    ShellPopup {
        id: popup
        visible: false
        screen: root.barWindow ? root.barWindow.screen : null
        anchorWindow: root.barWindow
        anchorItem: button
        popupWidth: 240
        popupHeight: Math.min(480, choicesScroll.height + contentPadding * 2)
        onVisibleChanged: if (visible && !root.preview && !WorkspaceState.available) visible = false

        Flickable {
            id: choicesScroll
            width: parent.width
            height: Math.min(480, choices.implicitHeight)
            contentHeight: choices.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}
            ColumnLayout {
                id: choices
                width: parent.width
                spacing: 2
                Repeater {
                    model: root.workspaces
                    ActionButton {
                        required property var modelData
                        text: modelData.number + "  " + modelData.name
                        iconName: "view-grid-symbolic"
                        flat: !modelData.active
                        alignLeft: true
                        Layout.fillWidth: true
                        onClicked: {
                            popup.visible = false;
                            if (!root.preview)
                                WorkspaceState.switchTo(modelData.id);
                        }
                    }
                }
            }
        }
    }
}
