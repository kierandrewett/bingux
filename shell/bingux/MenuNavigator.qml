import QtQuick

// Shared keyboard model for layer-shell menus. Pointer interaction stays
// lightweight; the keyboard highlight is only enabled after navigation with
// the arrow keys.
Item {
    id: root

    property var entries: []
    property Item view: null
    property Item focusTarget: null
    property int currentIndex: -1
    property bool keyboardNavigation: false
    readonly property var currentEntry: entryAt(root.view ? root.view.currentIndex : root.currentIndex)

    signal escapeRequested
    signal activateRequested(var entry)

    width: 0
    height: 0
    activeFocusOnTab: false

    function entryAt(index) {
        const source = root.entries;
        if (!source || index < 0)
            return null;
        if (root.view && index >= root.view.count)
            return null;
        if (!root.view) {
            if (typeof source.itemAt === "function")
                return index < source.count ? source.itemAt(index) : null;
            if (index >= source.length)
                return null;
        }
        return source[index] || null;
    }

    function entryCount() {
        if (root.view)
            return root.view.count;
        if (!root.entries)
            return 0;
        return typeof root.entries.count === "number" ? root.entries.count : root.entries.length;
    }

    function isNavigable(entry) {
        if (!entry || entry.visible === false || entry.enabled === false || entry.isSeparator === true)
            return false;
        // Non-ListView menus expose their visual children, so only explicit
        // menu entries are navigable. Model-backed menus use the model's
        // enabled/separator flags instead.
        return root.view ? true : entry.menuEntry === true;
    }

    function focusMenu() {
        root.keyboardNavigation = false;
        root.currentIndex = -1;
        if (root.view)
            root.view.currentIndex = -1;
        if (root.focusTarget)
            root.focusTarget.forceActiveFocus();
        else if (root.view)
            root.view.forceActiveFocus();
        else
            root.forceActiveFocus();
    }

    function pointerActivate() {
        root.keyboardNavigation = false;
        root.currentIndex = -1;
        if (root.view)
            root.view.currentIndex = -1;
    }

    function move(delta) {
        const count = root.entryCount();
        if (count <= 0 || delta === 0)
            return;

        root.keyboardNavigation = true;
        let index = root.view ? root.view.currentIndex : root.currentIndex;
        if (index < 0)
            index = delta > 0 ? -1 : 0;

        for (let step = 0; step < count; step++) {
            index = (index + delta + count) % count;
            const entry = root.entryAt(index);
            if (!root.isNavigable(entry))
                continue;

            root.currentIndex = index;
            if (root.view) {
                root.view.currentIndex = index;
                root.view.positionViewAtIndex(index, ListView.Contain);
                root.view.forceActiveFocus();
            } else if (entry && typeof entry.forceActiveFocus === "function") {
                entry.forceActiveFocus();
            }
            return;
        }
    }

    function activateCurrent() {
        const entry = root.entryAt(root.view ? root.view.currentIndex : root.currentIndex);
        if (root.isNavigable(entry))
            root.activateRequested(entry);
    }

    Keys.priority: Keys.BeforeItem
    Keys.onDownPressed: function (event) {
        root.move(1);
        event.accepted = true;
    }
    Keys.onUpPressed: function (event) {
        root.move(-1);
        event.accepted = true;
    }
    Keys.onReturnPressed: function (event) {
        root.activateCurrent();
        event.accepted = true;
    }
    Keys.onSpacePressed: function (event) {
        root.activateCurrent();
        event.accepted = true;
    }
    Keys.onEscapePressed: function (event) {
        root.escapeRequested();
        event.accepted = true;
    }
}
