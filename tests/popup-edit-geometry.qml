import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    PanelWindow { id: host; implicitWidth: 100; implicitHeight: 100 }
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
            entries: []
        }
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
        function initTestCase() { report.setText("RUNNING"); }
        function cleanupTestCase() { popup.visible = false; report.setText("FAILURES " + qtest_results.failCount); }
        function test_drop_origin_follows_popup_scale() {
            try {
                popup.visible = true;
                tryCompare(popup, "revealScale", 1, 3000);
                verify(waitForRendering(popup.body));
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
