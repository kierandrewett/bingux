import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Services.Mpris

ShellRoot {
    property string checks: ""
    FileView { id: results; path: Quickshell.env("BINGUX_MEDIA_TEST_RESULTS") }
    NotificationState { id: notificationState }
    NotificationSurface { id: notificationSurface; state: notificationState; notificationCentre: centre }
    ShellPopup {
        id: centre
        hostItem: notificationSurface.desktopViewport
        readonly property real listHeight: 300
        readonly property real listX: panelX + contentPadding
        readonly property real listY: panelY + contentPadding
        popupWidth: Theme.notificationWidth + contentPadding * 2
        popupHeight: 320
    }
    Dock {
        id: dock
        settings: QtObject { property var pinnedApps: ["dock-media-test", "no-media"] }
        notifications: notificationState.allEntries
        notificationStore: notificationState
        activity: QtObject { property var activeStreams: [] }
    }
    TestCase {
        id: test
        parent: dock.contentItem
        name: "DockMedia"
        when: true
        function check(value, message) {
            checks += message + ": " + value + "\n";
            results.setText(checks);
            verify(value, message);
        }
        function test_controls() {
            tryVerify(() => Mpris.players.values.length === 2 && dock.testItems.count === 2, 4000);
            const menu = dock.testItems.itemAt(0).testMenu;
            const unrelated = dock.testItems.itemAt(1).testMenu;
            tryVerify(() => menu.mediaPlayers.length === 1, 2000);
            check(unrelated.mediaPlayers.length === 0, "Unrelated app has no media controls");
            menu.visible = true;
            tryVerify(() => menu.revealScale === 1 && menu.body.parent.opacity === 1, 1000);
            test.parent = menu.contentItem;
            const play = findChild(menu.body, "mediaPlayPause");
            const next = findChild(menu.body, "mediaNext");
            const previous = findChild(menu.body, "mediaPrevious");
            const seek = findChild(menu.body, "mediaSeek");
            const title = findChild(menu.body, "mediaTitle");
            const player = menu.mediaPlayers[0];
            const elapsed = findChild(menu.body, "mediaElapsed");
            check(play !== null && next !== null && seek !== null, "Real dock menu contains the media section");
            check(title.text === "Test track" && title.textFormat === Text.PlainText, "Track metadata is plain text");
            check(play.text === "Play", "Paused player offers Play");
            const artButton = findChild(menu.body, "mediaArtButton");
            const section = findChild(menu.body, "mediaControls");
            mouseClick(artButton);
            wait(100);
            check(Theme.reducedMotion ? section.artExpansion === 1 : section.artExpansion > 0 && section.artExpansion < 1, "Artwork expansion respects motion preference");
            tryCompare(section, "artExpansion", 1);
            check(artButton.width === section.expandedArtSize && artButton.height === artButton.width, "Expanded artwork fills the padded media card as a square");
            mouseClick(artButton);
            tryCompare(section, "artExpansion", 0);
            check(artButton.width === section.smallArtSize, "Artwork animates back to the compact size");
            waitForRendering(seek);
            wait(100); // Let the resized layer surface receive its configure event.
            check(elapsed.width < elapsed.parent.width && elapsed.x === 0 && findChild(elapsed, "rollingGlyph0").x === 0,
                "Time toggle fits its text and stays left aligned");
            mouseMove(seek, seek.handle.width / 2 + (seek.availableWidth - seek.handle.width) / 4, seek.height / 2);
            wait(100);
            check(seek.previewVisible, "Seek hover preview is visible (slider hovered " + seek.hovered + ")");
            check(Math.abs(seek.previewPosition * seek.to - 45) < 1 && player.position === 0, "Hover previews time without seeking");
            const previewX = seek.previewPosition;
            mouseMove(seek, seek.handle.width / 2 + (seek.availableWidth - seek.handle.width) * .75, seek.height / 2);
            tryVerify(() => Math.abs(seek.previewPosition * seek.to - 135) < 1, 1000);
            check(seek.previewPosition > previewX, "Seek preview follows the pointer");
            mouseClick(elapsed);
            check(elapsed.text === "-3:00", "Elapsed time toggles to remaining time");
            mouseClick(elapsed);
            check(elapsed.text === "0:00", "Remaining time toggles back to elapsed");
            mouseClick(play);
            tryCompare(player, "isPlaying", true);
            const glyph = findChild(play, "mediaPlayPauseGlyph");
            tryCompare(glyph, "progress", 0);
            check(glyph.width === 24 && play.width === 40, "Play/pause morph uses the larger icon without enlarging the button");
            check(play.text === "Pause", "Playback update changes the button to Pause");
            check(menu.visible, "Playback controls keep the menu open");
            mouseClick(play);
            tryCompare(player, "isPlaying", false);
            tryCompare(glyph, "progress", 1);
            test.parent = menu.contentItem;
            play.forceActiveFocus();
            keyClick(Qt.Key_Space);
            tryCompare(player, "isPlaying", true);
            keyClick(Qt.Key_Space);
            tryCompare(player, "isPlaying", false);
            check(menu.visible, "Keyboard playback keeps the menu open");
            mouseClick(next);
            tryCompare(title, "text", "Second track");
            tryCompare(next, "enabled", false);
            check(previous.enabled, "Capabilities update after changing track");
            mouseClick(next);
            mouseClick(previous);
            tryCompare(title, "text", "Test track");
            tryCompare(next, "enabled", true);
            check(seek.enabled && seek.to === 180, "Seek range uses the reported duration");
            mouseClick(seek, seek.width / 2, seek.height / 2);
            tryVerify(() => Math.abs(player.position - 90) < 5, 2000);
            check(menu.visible, "Seeking keeps the menu open");
            const position = player.position;
            seek.forceActiveFocus();
            keyClick(Qt.Key_Right);
            tryVerify(() => player.position > position, 2000);
            check(menu.visible, "Keyboard seeking keeps the menu open");
            const beforeScroll = player.position;
            mouseWheel(seek, seek.width / 2, seek.height / 2, 0, 120);
            tryVerify(() => Math.abs(player.position - beforeScroll - 5) < 0.1, 2000);
            check(menu.visible, "Scrolling seeks five seconds without closing the menu");
            mouseWheel(seek, seek.width / 2, seek.height / 2, 0, 120);
            mouseWheel(seek, seek.width / 2, seek.height / 2, 0, 120);
            tryVerify(() => Math.abs(player.position - beforeScroll - 15) < 0.1, 2000);
            tryVerify(() => Math.abs(seek.value - player.position) < 0.1, 1000);
            check(true, "Rapid wheel steps accumulate and ease to the final position");
            menu.visible = false;
            tryVerify(() => !menu.closing, 1000);
            menu.visible = true;
            tryVerify(() => menu.revealScale === 1, 1000);
            const art = findChild(menu.body, "mediaArtwork");
            tryCompare(art, "status", Image.Ready);
            check(AlbumArtCache.sources.length === 1, "Reopened menu retains its artwork in memory");
            player.quit();
            tryVerify(() => menu.mediaPlayers.length === 0, 2000);
            check(findChild(menu.body, "mediaPlayPause") === null, "A disconnected player removes its controls");
            check(menu.visible, "Player removal preserves the app menu");
            menu.visible = false;
        }
        function test_badges() {
            if (Quickshell.env("BINGUX_MEDIA_TEST_ONLY") === "1") {
                skip("Notification and audio badges are outside the shared-player check");
                return;
            }
            tryVerify(() => dock.testItems.count === 2, 4000);
            const app = dock.testItems.itemAt(0);
            const other = dock.testItems.itemAt(1);
            const badge = findChild(app, "dockNotificationBadge");
            const number = findChild(badge, "dockBadgeCount");
            for (let index = 0; index < 2; index++) {
                Quickshell.execDetached(["notify-send", "--app-name=dock-media-test", "--hint=string:desktop-entry:dock-media-test", "--expire-time=2500", "Badge test " + index, "A retained notification preview"]);
                tryCompare(app, "notificationCount", index + 1, 3000);
            }
            check(Theme.reducedMotion || number.animating, "New notifications animate the count up");
            tryCompare(app, "notificationCount", 2, 3000);
            check(badge.count === 2 && other.notificationCount === 0, "Real notifications badge only their owning application");
            const toastTimeout = Math.max(...notificationState.visibleEntries.map(entry => entry.timeoutMs)) + 1000;
            tryVerify(() => notificationState.visibleEntries.length === 0, toastTimeout);
            check(app.notificationCount === 2 && notificationState.allEntries.length === 2, "Toast expiry retains notifications and badge counts");
            tryCompare(notificationSurface, "renderedNotificationCount", 0);
            centre.visible = true;
            tryCompare(notificationSurface, "renderedNotificationCount", 2);
            check(notificationSurface.notificationCount === 2, "Control centre restores archived notification cards");
            tryCompare(centre, "revealScale", 1);
            centre.visible = false;
            tryCompare(centre, "retained", false);
            tryCompare(notificationSurface, "renderedNotificationCount", 0);
            check(notificationState.allEntries.length === 2, "Closing the control centre hides history without clearing it");
            tryCompare(number, "displayedValue", 2);
            const menu = app.testMenu;
            menu.visible = true;
            tryCompare(menu, "revealScale", 1);
            test.parent = menu.contentItem;
            const previews = findChild(menu.body, "dockNotifications");
            check(previews.visible && previews.entries.length === 2, "Dock menu previews retained notifications for its app");
            const otherPreviews = findChild(other.testMenu.body, "dockNotifications");
            check(!otherPreviews.visible && otherPreviews.entries.length === 0, "Other apps do not show notification previews");
            waitForRendering(previews);
            wait(150);
            if (Quickshell.env("BINGUX_NOTIFICATION_PREVIEW_IMAGE")) {
                menu.body.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_NOTIFICATION_PREVIEW_IMAGE")));
                wait(100);
            }
            const card = findChild(previews, "notificationCard");
            tryCompare(card, "slideOffset", 0);
            check(card.groupCount === 1 && !card.collapsedStack, "Dock shows notifications as individual cards");
            check(card.radius === Theme.menuWidgetRadius && card.contentPadding === Theme.notificationPadding,
                "Dock notification card uses the shared menu radius and notification spacing");
            check(card.border.width === 0 && !card.layer.enabled && String(card.color) === String(Theme.menuWidgetBackground),
                "Dock widgets share the media background without borders or shadows");
            check(!previews.animationsEnabled && previews.cardMotion === 0 && previews.groupMotion === 0
                && card.slideOffset === 0 && card.entranceOpacity === 1,
                "Dock notifications appear immediately without slide or fade effects");
            const cards = Array.from(card.parent.children).filter(item => item.objectName === "notificationCard");
            check(cards.length === 2 && cards[1].y >= card.height && cards[1].width === card.width,
                "App notifications have equal widths and a flat vertical layout");
            check(!findChild(card, "notificationGroupToggle").visible && previews.bottomInset === 0,
                "Dock has no grouping control or extra space below notifications");
            mousePress(card, 80, 60);
            mouseMove(card, 100, 60, 20);
            mouseMove(card, 180, 60, 20);
            mouseRelease(card, 180, 60);
            check(card.dragOffset === 0 && !card.dismissing && previews.entries.length === 2,
                "Dragging a dock notification does not move or dismiss it");
            mouseMove(card, 100, 60);
            const clear = findChild(card, "notificationCloseButton");
            tryCompare(clear, "opacity", 1);
            const originalCardHeight = card.height;
            const originalMenuHeight = menu.popupHeight;
            const frames = Quickshell.env("BINGUX_NOTIFICATION_COLLAPSE_IMAGES");
            if (frames) { menu.body.grabToImage(result => result.saveToFile(frames + "-before.png")); wait(30); }
            mouseClick(clear, clear.width / 2, clear.height / 2);
            if (!Theme.reducedMotion) {
                tryVerify(() => card.collapseProgress > 0 && card.collapseProgress < 1 && menu.popupHeight < originalMenuHeight, 1000);
                check(card.height > 0 && card.height < originalCardHeight && card.slideOffset === 0,
                    "X dismisses through vertical collapse without sideways movement");
                check(menu.popupHeight < originalMenuHeight && menu.popupHeight > originalMenuHeight - originalCardHeight,
                    "Menu height follows the card collapse in flight");
                if (frames) menu.body.grabToImage(result => result.saveToFile(frames + "-during.png"));
            }
            tryCompare(app, "notificationCount", 1);
            check(Theme.reducedMotion || number.animating, "Clearing a preview animates the count down");
            tryCompare(number, "displayedValue", 1);
            check(previews.entries.length === 1 && menu.visible, "Clearing a preview updates the open menu");
            if (frames) { menu.body.grabToImage(result => result.saveToFile(frames + "-after.png")); wait(30); }
            notificationState.dismissAll();
            tryCompare(app, "notificationCount", 0);
            check(!badge.shown, "Dismissing notifications clears the count");
            menu.visible = false;
            dock.activity.activeStreams = [{properties: {"application.id": "dock-media-test"}}];
            const audio = findChild(app, "dockAudioBadge");
            tryCompare(audio, "opacity", 1);
            check(app.playingAudio && !other.playingAudio, "Audio activity badges only the matching application");
            dock.activity.activeStreams = [];
            wait(500);
            check(Theme.reducedMotion ? audio.opacity === 0 : audio.opacity > 0 && audio.opacity < 1, "Speaker badge respects fade timing and reduced motion");
            tryCompare(audio, "opacity", 0, 2000);
        }
        function test_discordIcon() {
            Quickshell.execDetached(["notify-send", "--app-name=Discord", "--hint=string:desktop-entry:discord", "--icon=discord", "--expire-time=0", "Icon resolution test"]);
            tryVerify(() => notificationState.allEntries.some(entry => entry.desktopEntry === "discord"), 3000);
            tryVerify(() => notificationState.allEntries.find(entry => entry.desktopEntry === "discord")?.appIcon === Quickshell.env("BINGUX_DISCORD_TEST_ICON"), 2000);
            const entry = notificationState.allEntries.find(entry => entry.desktopEntry === "discord");
            check(entry.appIcon === Quickshell.env("BINGUX_DISCORD_TEST_ICON"), "Discord's window identity resolves to the installed application icon");
            notificationState.dismiss(entry.notification);
        }
        function cleanupTestCase() {
            results.setText(checks + "FAILURES " + qtest_results.failCount + "\n");
            Qt.quit();
        }
    }
}
