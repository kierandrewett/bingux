import QtQuick
import Quickshell
import Quickshell.Wayland

SearchOverlay {
    id: preview
    visible: true
    WlrLayershell.namespace: "bingux-search-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    property real initialY: 0
    function find(item, name) {
        if (item.objectName === name)
            return item;
        for (const child of item.children || []) {
            const found = find(child, name);
            if (found)
                return found;
        }
        return null;
    }
    function check(condition, message) {
        if (!condition) {
            console.error("FAIL: " + message);
            Qt.exit(1);
        }
    }
    Timer {
        interval: 200
        running: true
        onTriggered: {
            const input = preview.find(preview.contentItem, "searchInput");
            input.text = "Fi";
            input.cursorPosition = input.length;
            preview.activeRequestId = "";
            preview.awaitingResults = false;
            const items = [];
            for (let i = 0; i < 20; i++)
                items.push({
                    resultId: "test-" + i,
                    providerId: "applications",
                    kind: "application",
                    title: "Files " + i,
                    subtitle: "Navigation test",
                    icon: "system-file-manager"
                });
            preview.displayedResults = items;
            preview.selectedIndex = 0;
        }
    }
    Timer {
        interval: 350
        running: true
        onTriggered: {
            const hint = preview.find(preview.contentItem, "searchSelectionHint");
            preview.check(hint.text === "les 0" && hint.visible, "selected name appears as faint suffix");
            preview.initialY = preview.find(preview.contentItem, "searchResultsList").contentY;
            preview.moveSelection(19);
            preview.check(Theme.reducedMotion || preview.find(preview.contentItem, "searchResultsList").contentY === preview.initialY, "keyboard scroll starts without jumping");
            preview.check(preview.find(preview.contentItem, "searchInput").text === "Fi", "hint leaves the query untouched");
        }
    }
    Timer {
        interval: 600
        running: true
        onTriggered: {
            const list = preview.find(preview.contentItem, "searchResultsList");
            preview.check(list.contentY > preview.initialY, "keyboard navigation scrolls down");
            const row = list.itemAtIndex(19);
            preview.check(row && row.y >= list.contentY - 1 && row.y + row.height <= list.contentY + list.height + 1, "selected row is fully visible");
            preview.moveSelection(1);
        }
    }
    Timer {
        interval: 900
        running: true
        onTriggered: {
            preview.check(Math.abs(preview.find(preview.contentItem, "searchResultsList").contentY - preview.initialY) < 1, "wrapping returns smoothly to the first row");
            const input = preview.find(preview.contentItem, "searchInput");
            input.cursorPosition = 1;
            preview.check(!preview.find(preview.contentItem, "searchSelectionHint").visible, "mid-query editing hides the hint");
            preview.completeSelectedName();
            preview.check(input.text === "Files 0" && input.cursorPosition === input.length, "Tab completion fills the app name and moves the caret to the end");
            preview.check(!preview.activationPending, "completion does not launch the result");
            Qt.quit();
        }
    }
}
