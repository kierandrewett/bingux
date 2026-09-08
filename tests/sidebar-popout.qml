import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: report; path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS") }
    Timer { id: finish; interval: 200; onTriggered: Qt.quit() }
    QtObject { id: preferences; property bool sidebarEnabled: true; property bool dockEnabled: false }
    TerminalSidebar { id: sidebar; settings: preferences; screen: Quickshell.screens[0]; inputSuspended: true }
    TestCase {
        parent: sidebar.detachedSurface.contentItem
        property string checks: ""
        function check(value, message) { checks += "Check " + value + " " + (message || "") + "\n"; verify(value,message); }
        function equal(a,b) { checks += "Compare " + a + " / " + b + "\n"; compare(a,b); }
        when: true
        function test_popout_preserves_content() {
            sidebar.selectContent("notes");
            const notes = findChild(sidebar.contentItem, "notesEditor");
            notes.text = "# A note\n\nKeep this text.";
            const content = sidebar.contentItem;
            const originalParent = content.parent;
            sidebar.popOut();
            wait(300);
            const window = sidebar.detachedSurface;
            check(sidebar.detached && window.visible, "detached window is mapped");
            equal(content.parent, window.contentItem);
            equal(findChild(sidebar.contentItem, "notesEditor"), notes);
            equal(notes.getText(0, notes.length), "A note\u2029Keep this text.");
            notes.cursorPosition = notes.length;
            notes.forceActiveFocus();
            keyClick(Qt.Key_Exclam);
            equal(notes.getText(0, notes.length), "A note\u2029Keep this text.!");
            equal(notes.leftPadding, 4);
            window.implicitWidth = 460;
            window.implicitHeight = 560;
            wait(100);
            if (Quickshell.env("BINGUX_POPOUT_SCREENSHOT")) grabImage(window.contentItem).save(Quickshell.env("BINGUX_POPOUT_SCREENSHOT"));
            const picker = findChild(sidebar.contentItem, "sidebarContentPicker");
            mouseClick(picker);
            wait(250);
            const menu = findChild(window.contentItem, "sidebarContentMenu");
            check(menu !== null, "detached content menu belongs to its own window");
            const anchor = picker.mapToItem(window.contentItem, 0, picker.height);
            const position = menu.mapToItem(window.contentItem, 0, 0);
            equal(position.x, anchor.x + Theme.spaceSmall);
            equal(position.y, anchor.y + Theme.spaceSmall * 2);
            mouseClick(window.contentItem, window.width - 16, window.height - 16);
            wait(150);
            sidebar.hide();
            // Exercise docking without reserving the user's desktop work area.
            preferences.sidebarEnabled = false;
            sidebar.dockBack();
            wait(200);
            equal(content.parent, originalParent);
            equal(findChild(sidebar.contentItem, "notesEditor"), notes);
            check(notes.getText(0, notes.length).endsWith("!"), "docking preserves the same editor");
            preferences.sidebarEnabled = true;
            sidebar.popOut();
            sidebar.selectContent("terminal");
            wait(600);
            check(sidebar.terminalReady, "real terminal loads");
            const terminal = findChild(sidebar.contentItem, "sidebarTerminal");
            check(terminal !== null);
            const pid = terminal.shellPid;
            check(pid > 0, "terminal process is running");
            sidebar.hide();
            preferences.sidebarEnabled = false;
            sidebar.dockBack();
            preferences.sidebarEnabled = true;
            sidebar.popOut();
            wait(300);
            equal(terminal.shellPid, pid);
            equal(findChild(sidebar.contentItem, "sidebarTerminal"), terminal);
            sidebar.hide();
        }
        function cleanupTestCase() { report.setText("FAILURES " + qtest_results.failCount + "\n" + checks); finish.start(); }
    }
}
