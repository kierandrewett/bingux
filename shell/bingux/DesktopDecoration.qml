import QtQuick
import "DesktopLayout.js" as DesktopLayout

Item {
    id: root
    required property string widgetId
    readonly property var spec: DesktopLayout.widget(widgetId) || {}
    readonly property var presentation: DesktopLayout.presentation(DesktopEditing.desktop, widgetId,
        DesktopLayout.placement(DesktopEditing.desktop, widgetId), spec.label, spec.icon,
        widgetId.startsWith("icon"), widgetId.startsWith("label"))
    implicitWidth: face.implicitWidth + Theme.barControlPadding * 2
    implicitHeight: Theme.barHeight
    Accessible.role: widgetId.startsWith("label") ? Accessible.StaticText : Accessible.Graphic
    Accessible.name: presentation.label
    WidgetFace { id: face; anchors.centerIn: parent; presentation: root.presentation }
}
