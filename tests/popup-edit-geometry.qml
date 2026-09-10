import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    PanelWindow { id: host; implicitWidth: 100; implicitHeight: 100 }
    QtObject {
        id: editor
        property bool visible: false
        property var desktop: BinguxPreferences.data.desktop
        property var layout: ({})
        property var dockApplications: []
        property string hoverZone: ""
        property string selectedContainer: ""
        property string optionsPage: ""
    }
    ShellPopup {
        id: popup
        screen: host.screen
        popupWidth: 400; popupHeight: 240
        preferredX: 80; preferredY: 80
        NativeEditSurface {
            id: surface
            parent: popup.body.parent
            anchors.fill: parent
            window: popup.nativeWindow
            zoneName: "control-centre"
            entries: [{id: "test-item", item: testItem}]
        }
        Item { id: testItem; width: 40; height: 40 }
    }
    QtObject {
        id: sharedMotion
        property bool retained: true
        property real revealScale: 1
        property real panelX: popup.panelX
        property real panelY: popup.panelY
        property real revealOriginX: popup.revealOriginX
        property real revealOriginY: popup.revealOriginY
        property var body: ({parent: {opacity: 1}})
    }
    TestCase {
        parent: host.contentItem
        when: host.visible
        function initTestCase() { report.setText("RUNNING"); DesktopEditing.editor = editor; }
        function cleanupTestCase() { DesktopEditing.editor = null; popup.visible = false; report.setText("FAILURES " + qtest_results.failCount); }
        function test_inactive_editor_releases_geometry() {
            editor.visible = false;
            compare(surface.expandedEntries.length, 0);
            compare(surface.screenRect.width, 0);
            editor.visible = true;
            tryCompare(surface.expandedEntries, "length", 1);
            verify(surface.screenRect.width > 0);
            editor.visible = false;
            compare(surface.expandedEntries.length, 0);
            compare(surface.screenRect.width, 0);
        }
        function test_drop_origin_follows_popup_scale() {
            try {
                editor.visible = true;
                popup.visible = true;
                tryCompare(popup, "revealScale", 1, 3000);
                tryCompare(popup.nativeWindow, "visible", true);
                for (const motion of [popup, sharedMotion]) {
                    popup.motionSource = motion === popup ? null : sharedMotion;
                    for (const scale of [0.7, 0.9, 1]) {
                        motion.revealScale = scale;
                        const expectedX = popup.panelX + popup.revealOriginX * (1 - scale);
                        const expectedY = popup.panelY + popup.revealOriginY * (1 - scale);
                        tryVerify(() => Math.abs(surface.screenRect.x - expectedX) < 0.01, 1000,
                            "The drop origin follows the actual popup transform");
                        compare(surface.screenRect.y, expectedY);
                        verify(Math.abs(surface.screenRect.width - popup.body.parent.width * scale) < 0.01,
                            "The drop width follows the visible card");
                        verify(Math.abs(surface.screenRect.height - popup.body.parent.height * scale) < 0.01,
                            "The drop height follows the visible card");
                    }
                }
                popup.motionSource = null;
            } catch (error) {
                console.error("POPUP_GEOMETRY_FAILED", error.message, error.stack);
                throw error;
            }
        }
    }
}
