import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io
import "DesktopLayout.js" as DesktopLayout
import "ControlLayout.js" as ControlLayout

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
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
        function stopSharing() {
            stops++;
            screenSharing = false;
        }
    }
    QtObject {
        id: metrics
        property bool screenSharing: false
        property bool microphoneInUse: true
        property bool locationInUse: true
    }
    PanelWindow {
        id: panel
        implicitWidth: 900
        implicitHeight: 160
        color: Theme.background
        Item {
            id: panelHost
            width: 208
            height: 500
        }
        PrivacyIndicators {
            id: indicators
            x: 30
            y: 50
            systemMetrics: metrics
            privacyState: privacy
            barWindow: panel
        }
        TestCase {
            when: panel.visible
            function initTestCase() {
                report.setText("RUNNING\n");
                DesktopEditing.editor = editor;
            }
            function cleanupTestCase() {
                DesktopEditing.editor = null;
                report.setText("FAILURES " + qtest_results.failCount);
            }
            function test_panel_fit_data() {
                return ["sidebar", "control-centre"].map(container => ({
                            tag: container,
                            container
                        }));
            }
            function test_panel_fit(data) {
                indicators.parent = panelHost;
                indicators.x = 0;
                indicators.y = 0;
                editor.layout = data.container === "sidebar" ? {
                    sidebar: ["privacy"]
                } : {};
                editor.desktop = {
                    controlLayout: ControlLayout.move(ControlLayout.defaults(), "control-centre", "privacy", 0),
                    containers: {
                        [data.container]: {
                            display: "both"
                        }
                    }
                };
                try {
                    const children = ["screenSharingIndicator", "cameraIndicator", "microphoneIndicator", "locationIndicator"].map(name => findChild(indicators, name));
                    for (const label of ["", "A deliberately long privacy indicator label to check clipping"]) {
                        editor.desktop = Object.assign({}, editor.desktop, {
                            widgetOptions: label ? {
                                privacy: {
                                    label
                                }
                            } : {}
                        });
                        verify(waitForRendering(indicators));
                        tryVerify(() => indicators.width <= panelHost.width, 1000, "Privacy fits its panel container");
                        for (const child of children) {
                            tryVerify(() => {
                                const point = child.mapToItem(indicators, 0, 0);
                                return point.x >= 0 && point.x + child.width <= indicators.width + 1 && point.y + child.height <= indicators.height + 1;
                            }, 1000, child.objectName + " remains inside the panel after wrapping");
                            const text = findChild(child, "widgetFaceLabel");
                            tryVerify(() => text.mapToItem(child, text.width, 0).x <= child.width, 1000, "The custom label fits its indicator");
                            if (label)
                                verify(text.truncated, "A long custom label ends with an ellipsis");
                        }
                        verify(indicators.height > Theme.barHeight, "Long indicators wrap into rows");
                        const capture = Quickshell.env("BINGUX_PRIVACY_CAPTURE");
                        if (capture && label) {
                            let saved = false;
                            verify(indicators.grabToImage(result => {
                                saved = result.saveToFile(capture + "-" + data.container + ".png");
                            }));
                            tryVerify(() => saved, 2000);
                        }
                    }
                } catch (error) {
                    console.error("PRIVACY_FIT_FAILED", error.message, error.stack);
                    throw error;
                } finally {
                    indicators.parent = panel.contentItem;
                    indicators.x = 30;
                    indicators.y = 50;
                }
            }
            function test_container_appearance_data() {
                return ["top-right", "dock", "sidebar", "control-centre"].map(container => ({
                            tag: container,
                            container
                        }));
            }
            function test_container_appearance(data) {
                editor.layout = data.container === "control-centre" ? {} : {
                    [data.container]: ["privacy"]
                };
                editor.desktop = {
                    controlLayout: ControlLayout.move(ControlLayout.defaults(), "control-centre", "privacy", data.container === "control-centre" ? 0 : -1),
                    containers: {
                        [data.container]: {
                            display: "text"
                        }
                    },
                    widgetOptions: {}
                };
                const children = ["screenSharingIndicator", "cameraIndicator", "microphoneIndicator", "locationIndicator"].map(name => findChild(indicators, name));
                try {
                    for (const mode of ["text", "icons", "both", "native"]) {
                        editor.desktop = Object.assign({}, editor.desktop, {
                            containers: {
                                [data.container]: {
                                    display: mode
                                }
                            }
                        });
                        for (const item of children) {
                            verify(item.visible);
                            compare(item.presentation.mode, mode);
                            compare(item.presentation.showText, mode === "text" || mode === "both");
                            compare(item.presentation.showIcon, mode !== "text");
                        }
                    }
                    editor.desktop = Object.assign({}, editor.desktop, {
                        widgetOptions: {
                            privacy: {
                                display: "both",
                                label: "Private",
                                icon: "starred-symbolic"
                            }
                        }
                    });
                    for (const item of children) {
                        compare(item.presentation.label, "Private");
                        compare(item.presentation.icon, "starred-symbolic");
                    }
                    privacy.screenSharing = true;
                    verify(waitForRendering(children[0], 2000));
                    if (["top-right", "dock"].includes(data.container)) {
                        compare(indicators.height, Theme.barHeight, "Bar and dock indicators retain one row");
                        compare(indicators.width, children.reduce((sum, item) => sum + item.implicitWidth, 0) + (children.length - 1) * Theme.barControlGap);
                    }
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
