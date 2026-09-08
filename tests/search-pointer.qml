import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

SearchOverlay {
    id: preview
    visible: true
    WlrLayershell.namespace: "bingux-search-pointer-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    property int activationCount: 0
    property var activationPosition: null
    function activateResult(result, position) {
        activationCount++;
        activationPosition = position;
        animateActivation(result, position);
    }
    function find(item, name) {
        if (item.objectName === name) return item;
        for (const child of item.children || []) {
            const found = find(child, name);
            if (found) return found;
        }
        return null;
    }
    TestCase {
        parent: preview.contentItem
        name: "SearchPointer"
        when: preview.visible
        function equal(actual, expected, message) {
            console.warn(message + ': ' + actual + ' expected ' + expected);
            compare(actual, expected, message);
        }
        function test_growth() {
            wait(250);
            const items = [];
            for (let i = 0; i < 3; i++) items.push({ resultId: 'test-' + i, providerId: 'applications', kind: 'application', title: 'Test ' + i, subtitle: '', icon: 'system-file-manager' });
            preview.displayedResults = items;
            wait(250);
            const list = preview.find(preview.contentItem, 'searchResultsList');
            const row = list.itemAtIndex(2);
            const target = row.mapToItem(preview.contentItem, 80, row.height / 2);
            preview.displayedResults = [];
            wait(250);
            mouseMove(preview.contentItem, target.x - 10, target.y);
            mouseMove(preview.contentItem, target.x, target.y);
            preview.displayedResults = items;
            preview.selectedIndex = 0;
            preview.keyboardSelection = true;
            wait(30);
            if (!Theme.reducedMotion) equal(preview.pointerBlocked, true, 'growth blocks mouse events');
            mouseClick(preview.contentItem, target.x, target.y);
            equal(preview.activationCount, 0, 'click during growth is blocked');
            wait(250);
            equal(preview.selectedIndex, 0, 'stationary cursor does not select after growth');
            equal(preview.pointerResultIndex, -1, 'stationary cursor does not arm clicks');
            mouseClick(preview.contentItem, target.x, target.y);
            equal(preview.activationCount, 0, 'stationary click after growth is blocked');
            equal(!preview.closing, true, 'blocked clicks do not dismiss search');
            mouseMove(preview.contentItem, target.x + 2, target.y);
            wait(30);
            equal(preview.selectedIndex, 2, 'real movement selects hovered result');
            equal(preview.pointerResultIndex, 2, 'only hovered row accepts clicks');
            mouseClick(preview.contentItem, target.x + 2, target.y);
            equal(preview.activationCount, 1, 'deliberate click activates once');
            equal(Math.abs(preview.activationPosition.x - target.x - 2) < 1
                && Math.abs(preview.activationPosition.y - target.y) < 1, true, 'click coordinates reach the launch animation');
            const effect = preview.find(preview.contentItem, 'searchLaunchEffect');
            if (!Theme.reducedMotion) {
                equal(Math.abs(effect.x + effect.width / 2 - target.x - 2) < 1
                    && Math.abs(effect.y + effect.height / 2 - target.y) < 1, true, 'mouse launch pulse is centred on the click');
                preview.animateActivation(items[2]);
                const icon = list.itemAtIndex(2).resultItem.activationIcon;
                const iconOrigin = icon.mapToItem(preview.contentItem, 0, 0);
                equal(effect.x, iconOrigin.x, 'keyboard launch keeps the icon origin');
                equal(effect.y, iconOrigin.y, 'keyboard launch keeps the icon origin vertically');
            }
            preview.selectedIndex = 0;
            preview.keyboardSelection = true;
            wait(50);
            equal(preview.selectedIndex, 0, 'stationary cursor preserves keyboard selection');
            preview.visible = false;
            wait(30);
            preview.visible = true;
            wait(250);
            equal(preview.pointerResultIndex, -1, 'reopening requires fresh motion');
            console.warn('SEARCH_POINTER_PASS');
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
