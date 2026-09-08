import QtQuick
import QtQuick.Window
import QtQuick.Controls
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
        property var nativeWindow: window
        property bool visible: true
        property var desktop: ({})
        property var layout: ({})
    }
    PanelWindow {
        id: window
        implicitWidth: 700
        implicitHeight: 400
        color: Theme.background
        property int actions: 0
        property int edits: 0
        property string edited: ""
        RowLayout {
            id: row
            x: 30; y: 30
            IconButton {
                id: disabledButton
                enabled: false
                iconName: "network-wireless-symbolic"
                label: "Unavailable network"
                onClicked: window.actions++
                WidgetEditHandle { control: disabledButton; widgetId: "network"; onRequested: id => { window.edits++; window.edited = id; } }
            }
            IconButton {
                id: neighbour
                iconName: "starred-symbolic"
                label: "Neighbour"
                onClicked: window.actions++
                WidgetEditHandle { control: neighbour; widgetId: "neighbour"; onRequested: id => { window.edits++; window.edited = id; } }
            }
        }
        ActionButton { id: before; x: 30; y: 100; text: "Before preview" }
        Item { id: previewHost; x: 200; y: 100; width: 320; height: 240 }
        ActionButton { id: after; x: 550; y: 100; text: "After preview" }
        Component { id: previewComponent; WidgetPreview { anchors.fill: parent } }
        TestCase {
            when: window.visible
            function initTestCase() {
                report.setText("RUNNING\n");
                DesktopEditing.editor = editor;
                verify(waitForRendering(row));
                wait(100);
            }
            function cleanupTestCase() { DesktopEditing.editor = null; report.setText("FAILURES " + qtest_results.failCount); }
            function test_disabled_edit() {
                try {
                mouseClick(disabledButton, 16, 16, Qt.LeftButton);
                compare(window.actions, 0);
                mouseClick(disabledButton, 16, 16, Qt.RightButton);
                compare(window.edits, 0);
                mouseClick(disabledButton, 16, 16, Qt.RightButton, Qt.ShiftModifier);
                compare(window.edits, 1, "An unavailable control retains its edit menu");
                compare(window.edited, "network");
                mouseClick(neighbour, 16, 16, Qt.RightButton, Qt.ShiftModifier);
                compare(window.edits, 2, "The disabled neighbour must not receive this click");
                compare(window.edited, "neighbour");
                row.x += 100;
                mouseClick(disabledButton, 16, 16, Qt.RightButton, Qt.ShiftModifier);
                compare(window.edits, 3, "The disabled edit target follows its current geometry");
                disabledButton.enabled = true;
                mouseClick(disabledButton, 16, 16, Qt.RightButton, Qt.ShiftModifier);
                compare(window.edits, 4, "Enabling a widget must not create duplicate edit actions");
                mouseClick(disabledButton, 16, 16, Qt.LeftButton);
                compare(window.actions, 1);
                row.enabled = false;
                mouseClick(disabledButton, 16, 16, Qt.RightButton, Qt.ShiftModifier);
                compare(window.edits, 5, "An unavailable group retains editing for its children");
                disabledButton.visible = false;
                mouseClick(row, 16, 16, Qt.RightButton, Qt.ShiftModifier);
                compare(window.edits, 5, "Hidden widgets must not respond to editing gestures");
                row.enabled = true;
                disabledButton.visible = true;
                } catch (error) { console.error("EDIT_INPUT_FAILURE", error.stack); throw error; }
            }
            function test_preview_keyboard() {
                try {
                for (const spec of DesktopLayout.widgets.concat(DesktopLayout.controlWidgets, DesktopLayout.layoutWidgets, DesktopLayout.decorationWidgets, ControlLayout.widgets)) {
                    const id = spec.id;
                    before.forceActiveFocus();
                    tryCompare(before, "activeFocus", true, 1500);
                    const preview = previewComponent.createObject(previewHost, {widgetId: id});
                    tryVerify(() => preview.previewControl !== null, 1500);
                    wait(30);
                    verify(before.activeFocus, "Loading a preview must not steal keyboard focus");
                    for (let i = 0; i < 12; i++) {
                        keyClick(Qt.Key_Tab);
                        const focused = window.contentItem.Window.window.activeFocusItem;
                        verify(focused !== null, "Keyboard focus remains on the surrounding UI");
                        for (let item = focused; item; item = item.parent)
                            verify(item !== preview, "Tab must not enter the " + id + " preview");
                    }
                    preview.destroy(); wait(30);
                }
                } catch (error) { console.error("PREVIEW_INPUT_FAILURE", error.stack); throw error; }
            }
        }
    }
}
