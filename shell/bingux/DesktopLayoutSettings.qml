import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var settings
    readonly property var containers: [
        {
            id: "top-right",
            page: "TopBar",
            title: "Top bar",
            description: "Arrange status widgets, labels and spaces",
            icon: "view-dual-symbolic"
        },
        {
            id: "dock",
            page: "Dock",
            title: "Dock",
            description: "Arrange apps and widgets, size, alignment and mouse actions",
            icon: "view-app-grid-symbolic"
        },
        {
            id: "sidebar",
            page: "Sidebar",
            title: "Sidebar",
            description: "Arrange panels and choose the screen edge",
            icon: "sidebar-show-symbolic"
        },
        {
            id: "control-centre",
            page: "Controls",
            title: "Control centre",
            description: "Arrange controls and choose their appearance",
            icon: "preferences-system-symbolic"
        }
    ]
    readonly property var shownContainers: settings.page === "Desktop" ? containers : containers.filter(item => item.page === settings.page)
    Layout.fillWidth: true
    spacing: 24
    SettingsHeading {
        title: root.settings.page === "Desktop" ? "Desktop layout" : root.shownContainers[0]?.title || "Desktop layout"
        description: "Drag widgets into place and preview your changes in Customise UI."
    }
    SettingsGroup {
        Repeater {
            model: root.shownContainers
            ControlRow {
                required property var modelData
                objectName: "customise-container-" + modelData.id
                title: modelData.title
                subtitle: modelData.description
                iconName: modelData.icon
                navigation: true
                enabled: root.settings.ready && !root.settings.busy
                onClicked: root.settings.openContainerCustomise(modelData.id)
            }
        }
    }
    ActionButton {
        objectName: "customiseDesktop"
        text: "Customise desktop"
        iconName: "document-edit-symbolic"
        flat: true
        enabled: root.settings.ready && !root.settings.busy
        onClicked: root.settings.customiser.open()
    }
}
