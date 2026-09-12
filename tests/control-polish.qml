import QtQuick
import QtQuick.Layouts
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    FloatingWindow {
        id: window
        implicitWidth: 440
        implicitHeight: 410
        color: Theme.shellSurface
        Rectangle {
            id: canvas
            anchors.fill: parent
            color: Theme.shellSurface
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
                spacing: Theme.padding
                RowLayout {
                    Layout.fillWidth: true
                    spacing: Theme.gap
                    ControlRow {
                        id: compact
                        tileLayout: true
                        compactTile: true
                        rowInteractive: false
                        toggleVisible: true
                        title: "Do Not Disturb"
                        iconName: "notifications-disabled-symbolic"
                    }
                    ControlRow {
                        tileLayout: true
                        compactTile: true
                        rowInteractive: false
                        toggleVisible: true
                        selected: true
                        title: "Night Light"
                        iconName: "night-light-symbolic"
                    }
                }
                Text {
                    text: "Volume"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                }
                SeekSlider {
                    id: volume
                    Layout.fillWidth: true
                    from: 0
                    to: 1.5
                    value: 0.65
                    stepSize: 0.01
                    warningFrom: 1
                }
                Text {
                    text: "Playback"
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                }
                SeekSlider {
                    id: playback
                    Layout.fillWidth: true
                    from: 0
                    to: 240
                    value: 95
                }
                Text {
                    text: "Unavailable"
                    color: Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSmall
                }
                SeekSlider {
                    Layout.fillWidth: true
                    enabled: false
                    value: 0.35
                }
                Item {
                    Layout.fillHeight: true
                }
            }
        }
        TestCase {
            id: test
            property bool saved: false
            when: window.visible
            function test_slider() {
                report.setText("FAILURES 1\n");
                waitForRendering(volume);
                compare(compact.padding, 12);
                verify(compact.implicitHeight >= 84);
                verify(volume.height >= 36);
                const handleWidth = volume.handle.width;
                mouseMove(volume, volume.width / 2, volume.height / 2);
                wait(160);
                compare(volume.handle.width, handleWidth, "Hover never changes value geometry");
                compare(volume.background.height, Theme.sliderActiveTrackHeight);
                const previewBoost = findChild(volume, "sliderPreviewBoostFill");
                compare(previewBoost.width, 0, "Normal-volume hover has no red preview");
                const originalValue = volume.value;
                mouseMove(volume, volume.width * 0.9, volume.height / 2);
                wait(160);
                verify(previewBoost.width > 0 && previewBoost.opacity > 0, "Hovering above 100% previews the boost in red");
                compare(previewBoost.children[0].color, volume.warningColor);
                compare(volume.value, originalValue, "Preview does not change volume");
                fuzzyCompare(previewBoost.x, volume.warningPosition * volume.background.width, 0.01);
                compare(findChild(playback, "sliderPreviewBoostFill").width, 0, "Playback has no boost region");
                mouseMove(canvas, 2, 2);
                wait(160);
                compare(previewBoost.opacity, 0, "Boost preview disappears on pointer exit");
                compare(volume.background.height, Theme.sliderTrackHeight);
                mouseClick(volume, 0, volume.height / 2);
                compare(volume.value, 0, "Left edge reaches zero");
                mouseClick(volume, volume.width - 1, volume.height / 2);
                compare(volume.value, volume.to, "Right edge reaches maximum");
                verify(findChild(volume, "sliderBoostFill").width > 0);
                volume.muted = true;
                verify(!findChild(volume, "sliderBoostFill").visible, "Muted audio does not show boost colour");
                volume.muted = false;
                volume.forceActiveFocus(Qt.TabFocusReason);
                keyClick(Qt.Key_Left);
                verify(volume.value < volume.to, "Keyboard adjusts value");
                volume.enabled = false;
                const disabledValue = volume.value;
                mouseClick(volume, 0, volume.height / 2);
                compare(volume.value, disabledValue, "Disabled slider cannot change");
                volume.enabled = true;
                volume.leftPadding = 8;
                volume.rightPadding = 12;
                compare(volume.normalizedPositionAt(0), 0);
                compare(volume.normalizedPositionAt(volume.width), 1);
                volume.LayoutMirroring.enabled = true;
                compare(volume.normalizedPositionAt(0), 1, "RTL preview mapping");
                mouseClick(volume, volume.width - 1, volume.height / 2);
                compare(volume.value, 0, "RTL right endpoint");
                volume.LayoutMirroring.enabled = false;
                volume.leftPadding = 0;
                volume.rightPadding = 0;
                volume.value = 0.65;
                canvas.forceActiveFocus();
                mouseMove(volume, volume.width * 0.9, volume.height / 2);
                wait(160);
                canvas.grabToImage(result => {
                    test.saved = result.saveToFile("/tmp/bingux-control-polish.png");
                });
                tryCompare(test, "saved", true, 2000);
                report.setText("PASS padded tiles; slider hit area, stable thumb geometry, endpoints, boost/mute, keyboard, disabled and RTL\nFAILURES 0\n");
            }
        }
    }
}
