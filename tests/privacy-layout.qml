import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS") }
    QtObject {
        id: editor
        property bool visible: true
        property var desktop: ({})
        property var layout: ({})
        property var nativeWindow: panel
    }
    QtObject {
        id: privacy
        property bool available: true
        property bool screenSharing: true
        property bool cameraInUse: true
        property bool microphoneInUse: true
        property string microphoneTooltip: "Microphone in use"
        property int stops: 0
        function stopSharing() { stops++; screenSharing = false; }
    }
    QtObject {
        id: metrics
        property bool screenSharing: false
        property bool microphoneInUse: true
        property bool locationInUse: true
    }
    PanelWindow {
        id: panel
        implicitWidth: 900; implicitHeight: 160
        color: Theme.background
        PrivacyIndicators { id: indicators; x: 30; y: 50; systemMetrics: metrics; privacyState: privacy; barWindow: panel }
        TestCase {
            when: panel.visible
            function initTestCase() { report.setText("RUNNING\n"); DesktopEditing.editor = editor; }
            function cleanupTestCase() { DesktopEditing.editor = null; report.setText("FAILURES " + qtest_results.failCount); }
            function test_container_appearance_data() {
                return ["top-right", "dock", "sidebar", "control-centre"].map(container => ({tag: container, container}));
            }
            function test_container_appearance(data) {
                editor.layout = data.container === "control-centre" ? {} : {[data.container]: ["privacy"]};
                editor.desktop = {controlLayout: ControlLayout.move(ControlLayout.defaults(), "control-centre", "privacy", data.container === "control-centre" ? 0 : -1),
                    containers: {[data.container]: {display: "text"}}, widgetOptions: {}};
                const children = ["screenSharingIndicator", "cameraIndicator", "microphoneIndicator", "locationIndicator"]
                    .map(name => findChild(indicators, name));
                try {
                    for (const mode of ["text", "icons", "both", "native"]) {
                        editor.desktop = Object.assign({}, editor.desktop, {containers: {[data.container]: {display: mode}}});
                        for (const item of children) {
                            verify(item.visible);
                            compare(item.presentation.mode, mode);
                            compare(item.presentation.showText, mode === "text" || mode === "both");
                            compare(item.presentation.showIcon, mode !== "text");
                        }
                    }
                    editor.desktop = Object.assign({}, editor.desktop, {widgetOptions: {privacy: {display: "both", label: "Private", icon: "starred-symbolic"}}});
                    for (const item of children) {
                        compare(item.presentation.label, "Private"); compare(item.presentation.icon, "starred-symbolic");
                    }
                    privacy.screenSharing = true;
                    verify(waitForRendering(children[0], 2000));
                    const stopped = privacy.stops;
                    mouseClick(children[0], children[0].width / 2, children[0].height / 2);
                    compare(privacy.stops, stopped + 1, "The styled sharing control still dispatches its stop action");
                    verify(indicators.sharingVisible, "Stopping keeps the existing brief visibility hold");
                    privacy.screenSharing = true;
                } catch (error) {
                    console.error("PRIVACY_LAYOUT_FAILED", data.container, error.message, error.stack);
                    throw error;
                }
            }
        }
    }
}
