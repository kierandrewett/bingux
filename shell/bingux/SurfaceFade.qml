import QtQuick

Loader {
    id: root
    objectName: "surfaceFade"
    required property Item target
    property Item layerTarget: target
    // Qt otherwise applies inherited opacity to overlapping children one by
    // one. Fade the completed item texture, including its text and border.
    Binding {
        target: root.layerTarget
        property: "layer.enabled"
        when: root.target.visible && root.target.opacity > 0 && root.target.opacity < 1
        value: true
        restoreMode: Binding.RestoreBindingOrValue
    }
    sourceComponent: SurfaceFades.component.status === Component.Ready ? SurfaceFades.component : null
    onLoaded: item.target = Qt.binding(() => root.target)
}
