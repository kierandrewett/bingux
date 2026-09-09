import QtQuick

// Explicit orientation is required for a mouse's separate horizontal wheel.
WheelHandler {
    id: root
    required property Flickable viewport
    target: null
    orientation: Qt.Horizontal
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    acceptedModifiers: Qt.KeyboardModifierMask
    onWheel: event => {
        const delta = event.pixelDelta.x || event.angleDelta.x / 120 * 136;
        const extent = Math.max(0, viewport.contentWidth - viewport.width);
        if (!delta || extent === 0) {
            event.accepted = false;
            return;
        }
        viewport.cancelFlick();
        const origin = viewport.originX || 0;
        viewport.contentX = origin + Math.max(0, Math.min(extent, viewport.contentX - origin - delta));
        event.accepted = true;
    }
}
