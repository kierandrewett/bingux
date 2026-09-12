import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

SearchOverlay {
    id: preview
    visible: true
    WlrLayershell.namespace: "bingux-search-app-menu-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    SearchSocket {
        id: protocolCheck
    }
    dockView: testDock
    Dock {
        id: testDock
        visible: false
        settings: ({
                pinnedApps: []
            })
    }
    property int activationCount: 0
    function activateResult(result, position) {
        activationCount++;
    }
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
    TestCase {
        parent: preview.contentItem
        name: "SearchAppMenu"
        when: preview.visible
        function equal(actual, expected, message) {
            console.warn((message || 'check') + ': ' + actual + ' expected ' + expected);
            compare(actual, expected, message);
        }
        function test_menu() {
            wait(300);
            const result = {
                resultId: 'r300',
                desktopId: 'bingux-menu-test.desktop',
                providerId: 'applications',
                kind: 'application',
                title: 'Menu test app',
                subtitle: '',
                icon: 'application-x-executable',
                score: 1
            };
            verify(protocolCheck.isValidResult(result), 'Accept daemon app identity');
            preview.displayedResults = [result, Object.assign({}, result, {
                    resultId: "r301",
                    title: "Another app"
                })];
            preview.selectedIndex = 0;
            wait(300);
            const list = preview.find(preview.contentItem, 'searchResultsList');
            const row = list.itemAtIndex(0);
            const point = row.mapToItem(preview.contentItem, 80, row.height - 20);
            mouseMove(preview.contentItem, point.x - 5, point.y);
            mouseMove(preview.contentItem, point.x, point.y);
            wait(30);
            mouseClick(preview.contentItem, point.x, point.y, Qt.RightButton);
            const pin = preview.find(preview.contentItem, 'searchAppPinAction');
            verify(pin !== null);
            tryCompare(pin, 'visible', true);
            equal(preview.activationCount, 0, 'Right click must not launch the app');
            const unpin = Quickshell.env('BINGUX_MENU_UNPIN') === '1';
            equal(pin.text, unpin ? 'Unpin from dock' : 'Pin to dock');
            wait(250);
            const otherRow = list.itemAtIndex(1);
            const otherPoint = otherRow.mapToItem(preview.contentItem, otherRow.width - 30, otherRow.height - 20);
            mouseMove(preview.contentItem, otherPoint.x, otherPoint.y);
            wait(30);
            equal(preview.selectedIndex, 0, 'Context menu freezes the search highlight');
            mouseClick(preview.contentItem, otherPoint.x, otherPoint.y);
            wait(250);
            equal(pin.visible, false, 'Click on another result only dismisses the menu');
            equal(preview.activationCount, 0, 'Dismissal click does not activate the covered result');
            equal(preview.selectedIndex, 0, 'Dismissal preserves the previous highlight');
            mouseMove(preview.contentItem, otherPoint.x + 2, otherPoint.y);
            wait(30);
            equal(preview.selectedIndex, 1, 'Hover resumes after menu dismissal');
            mouseMove(preview.contentItem, point.x, point.y);
            wait(30);
            mouseClick(preview.contentItem, point.x, point.y, Qt.RightButton);
            wait(250);
            if (Quickshell.env('BINGUX_MENU_SCREENSHOT'))
                grabImage(pin.parent.parent.parent).save(Quickshell.env('BINGUX_MENU_SCREENSHOT') + (unpin ? '-unpin.png' : '-pin.png'));
            mouseClick(pin, pin.width / 2, pin.height / 2);
            tryVerify(() => testDock.isPinned({
                    id: result.desktopId
                }) === !unpin);
            equal(preview.visible, true, 'Pin action keeps search open');
            equal(preview.activationCount, 0, 'Pin action must not launch the app');
            wait(300);
            preview.openAppMenu(result, point);
            tryCompare(pin, 'visible', true);
            wait(250);
            keyClick(Qt.Key_Escape);
            wait(250);
            equal(pin.visible, false, 'Escape dismisses only the menu');
            equal(preview.visible, true);
            preview.openAppMenu(result, point);
            wait(250);
            mouseClick(preview.contentItem, 10, 10);
            wait(250);
            equal(pin.visible, false, 'Outside click dismisses only the menu');
            equal(preview.closing, false);
            preview.openAppMenu(result, point);
            preview.displayedResults = [];
            wait(250);
            equal(pin.visible, false, 'Changing results dismisses stale menu');
            preview.openAppMenu({
                providerId: 'files',
                kind: 'file'
            }, point);
            equal(pin.visible, false, 'Non-app results have no app menu');
            preview.displayedResults = [result];
            preview.selectedIndex = 0;
            wait(300);
            preview.focusSearchInput();
            wait(30);
            keyClick(Qt.Key_F10, Qt.ShiftModifier);
            tryCompare(pin, 'visible', true);
            wait(250);
            keyClick(Qt.Key_Down);
            keyClick(Qt.Key_Down);
            equal(pin.activeFocus, true, 'Menu supports arrow-key navigation');
            keyClick(Qt.Key_Escape);
            wait(500);
            console.warn('SEARCH_APP_MENU_PASS');
        }
    }
}
