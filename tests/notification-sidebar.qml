import QtQuick
import QtTest
import Quickshell

ShellRoot {
    QtObject {
        id: notification
        property int id: 100
        property real expireTimeout: 0
        property string appName: "Files"
        property string desktopEntry: ""
        property string appIcon: "system-file-manager"
        property string summary: "Download complete"
        property string body: "Notifications stay in the desktop area beside the sidebar."
        property var actions: []
        property bool tracked: false
        signal closed(int reason)
        function dismiss() {}
        function expire() {}
    }
    NotificationState { id: state }
    NotificationSurface {
        id: surface
        state: state
        notificationCentre: history
        sidebarScreen: Quickshell.screens[0]
        rightInset: 300
        Rectangle {
            x: surface.width - surface.rightInset
            width: surface.rightInset
            height: surface.height
            color: "#20363d"
            z: -1
            Text { anchors.centerIn: parent; text: "Sidebar"; color: "white" }
        }
    }
    NotificationHistoryPopup { id: history; notificationSurface: surface; screen: surface.screen }
    TestCase {
        parent: surface.contentItem
        name: "NotificationSidebar"
        when: true
        function check(value, message) { console.warn(message + ": " + value); verify(value, message); }
        function bounds() {
            const area = surface.desktopViewport;
            const viewport = surface.viewport;
            const point = viewport.mapToItem(surface.contentItem, 0, 0);
            check(point.x >= area.x && point.x + viewport.width <= area.x + area.width + .1,
                "Notification viewport stays inside the desktop boundary");
            check(area.clip, "Desktop boundary clips every slide animation");
        }
        function test_sidebar() {
            state.accept(notification);
            tryVerify(() => surface.renderedNotificationCount === 1 && surface.width > 600);
            const card = findChild(surface.contentItem, "notificationCard");
            waitForRendering(card);
            bounds();
            check(surface.desktopViewport.width === surface.width - 300, "Right sidebar is excluded from notification space");
            if (!Theme.reducedMotion) {
                check(card.slideOffset > 0, "Toast enters from beyond the desktop edge");
                for (let frame = 0; frame < 8; frame++) { wait(16); bounds(); }
            }
            tryCompare(card, "slideOffset", 0);
            const originalCard = card;
            history.visible = true;
            const clear = findChild(surface.contentItem, "controlClearNotifications");
            verify(clear);
            if (!Theme.reducedMotion) {
                check(history.slideOffset > 0, "The whole centre starts beyond the desktop edge");
                const startOffset = history.slideOffset;
                wait(50);
                check(history.slideOffset > 0 && history.slideOffset < startOffset, "Centre slides towards its final position");
                check(card.slideOffset === 0, "Cards do not animate their own entrance inside the centre");
                check(history.revealScale === 1, "Centre entrance does not scale");
                const footerRight = clear.mapToItem(surface.desktopViewport, clear.width, 0).x;
                check(Math.abs(footerRight - surface.viewport.x - history.body.width) < .1,
                    "Notification list and footer share the same slide offset");
            }
            tryCompare(history, "slideOffset", 0);
            tryCompare(history, "revealScale", 1);
            check(history.hostItem === surface.desktopViewport, "History and its dismissal area use the desktop viewport");
            check(findChild(surface.contentItem, "notificationCard") === originalCard, "Opening history retains the same notification card");
            bounds();
            check(history.panelX >= 0 && history.panelX + history.popupWidth <= surface.desktopViewport.width,
                "Notification centre stays left of the right sidebar");
            check(surface.mask.x === 0 && surface.mask.width === surface.width - 300,
                "History input mask excludes the sidebar");
            Quickshell.execDetached(["grim", "/tmp/bingux-notification-polish/notification-sidebar.png"]);
            wait(150);
            surface.rightInset = 420;
            tryVerify(() => surface.desktopViewport.width === surface.width - 420);
            waitForRendering(card);
            bounds();
            check(findChild(surface.contentItem, "notificationCard") === originalCard, "Resizing the sidebar preserves card state");
            surface.rightInset = 0;
            surface.leftInset = 300;
            tryVerify(() => surface.desktopViewport.x === 300);
            waitForRendering(card);
            bounds();
            check(surface.mask.x === 300 && surface.mask.width === surface.width - 300,
                "Left sidebar is excluded from history input and layout");
            surface.sidebarScreen = {name: "another-output"};
            tryVerify(() => surface.desktopViewport.x === 0 && surface.desktopViewport.width === surface.width);
            check(true, "A sidebar on another screen does not change notification bounds");
            keyClick(Qt.Key_Escape);
            if (!Theme.reducedMotion) {
                wait(150);
                check(history.retained && history.slideOffset > 0 && history.slideOffset < history.width - history.panelX,
                    "Centre remains mapped throughout its eased slide to the right");
                const footerRight = clear.mapToItem(surface.desktopViewport, clear.width, 0).x;
                check(Math.abs(footerRight - surface.viewport.x - history.body.width) < .1,
                    "List and footer slide out together");
            }
            tryVerify(() => !history.retained);
            check(state.allEntries.length === 1, "Closing history preserves the notification");
            tryVerify(() => !surface.visible, 2000, "Archived history must unmap the empty full-screen surface");
            history.visible = true;
            if (!Theme.reducedMotion) wait(50);
            history.visible = false;
            const interruptedOffset = history.slideOffset;
            wait(30);
            if (!Theme.reducedMotion) check(history.slideOffset > interruptedOffset,
                "Closing reverses an interrupted entrance towards the right");
            history.visible = true;
            tryCompare(history, "slideOffset", 0);
            check(history.visible && history.retained, "Reopening an interrupted entrance finishes the shared slide");
            history.visible = false;
            tryVerify(() => !history.retained);
            state.dismiss(notification);
            tryVerify(() => surface.renderedNotificationCount === 0);
            console.info("NOTIFICATION_SIDEBAR_PASSED");
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
