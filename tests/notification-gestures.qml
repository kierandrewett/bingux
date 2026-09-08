import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
ShellRoot {
    FileView { id: results; path: Quickshell.env("BINGUX_NOTIFICATION_TEST_RESULTS") }
    property string checks: ""
    Component {
        id: notificationFactory
        QtObject {
        property int id: 1
        property real expireTimeout: 0
        property string appName: "Sender"
        property string desktopEntry: "org.bingux.NotificationTest"
        property string appIcon: ""
        property string summary: "Download complete"
        property string body: "Drag right to dismiss this notification."
        property var actions: []
        property bool tracked: false
        signal closed(int reason)
        function dismiss() {}
        function expire() {}
        }
    }
    PanelWindow {
        id: testControls
        anchors.bottom: true
        anchors.right: true
        implicitWidth: 180
        implicitHeight: 40
        NotificationTestButton { id: spawnButton; anchors.fill: parent }
    }
    NotificationState { id: state }
    NotificationSurface {
        id: surface
        state: state
        TestCase {
            name: "NotificationGestures"
            when: true
            function check(value, message) { checks += "CHECK " + value + " " + (message || "") + "\n"; results.setText(checks); verify(value, message); }
            function equal(actual, expected, message) { checks += "EQUAL " + actual + " " + expected + " " + (message || "") + "\n"; results.setText(checks); compare(actual, expected, message); }
            function card() { return findChild(surface.contentItem, "notificationCard"); }
            function test_gestures() {
                // This suite exercises shared grouped-card gestures. Separate
                // incoming toasts and centre transitions are covered by the
                // notification-performance fixture.
                surface.viewport.groupNotifications = true;
                const notification = notificationFactory.createObject(surface);
                state.accept(notification);
                waitForRendering(card());
                const initialBackdrop = findChild(surface.contentItem, "notificationGroupBackground");
                check(!initialBackdrop.visible && initialBackdrop.opacity === 0, "unexpanded notifications never flash a raised background on creation");
                wait(60);
                check(card().slideOffset > 0 && card().slideOffset < card().width, "entrance is eased in flight: " + card().slideOffset + " / " + card().width);
                check(card().opacity > 0.65 && card().opacity < 1, "arriving card fades in slightly while sliding");
                const openingViewport = findChild(surface.contentItem, "notificationViewport");
                check(openingViewport.height >= card().y + card().height, "single notification viewport already contains its full height during entry: " + openingViewport.height + " / " + card().height);
                const openingGeometry = [card().y, card().width, card().height].join(",");
                for (let frame = 0; frame < 24; frame++) {
                    wait(16);
                    equal([card().y, card().width, card().height].join(","), openingGeometry, "opening card only moves horizontally");
                }
                tryCompare(card(), "slideOffset", 0, 1000);
                tryCompare(card(), "opacity", 1, 1000);
                check(card() !== null);
                equal(card().slideOffset, 0);
                equal(card().color.a, 1, "top notification background is fully opaque");
                equal(card().entry.appName, "Files", "desktop entry supplies the app name");
                check(card().entry.appIcon.length > 0, "desktop entry supplies the icon");
                if (Quickshell.env("BINGUX_NOTIFICATION_TEST_SCREENSHOT")) {
                    const position = card().mapToGlobal(0, 0);
                    Quickshell.execDetached(["grim", "-g", Math.round(position.x) + "," + Math.round(position.y) + " " + Math.round(card().width) + "x" + Math.round(card().height), Quickshell.env("BINGUX_NOTIFICATION_TEST_SCREENSHOT")]);
                    wait(300);
                }
                const firstCard = card();
                const overlay = notificationFactory.createObject(surface, {id: 900, summary: "Covering notification"});
                state.accept(overlay);
                wait(100);
                equal(firstCard.contentsOpacity, 1, "previous contents remain visible while the new card slides over them");
                const incoming = Array.from(firstCard.parent.children).find(item => item.objectName === "notificationCard" && item.notificationId === 900);
                tryCompare(incoming, "slideOffset", 0, 1000);
                wait(40);
                check(firstCard.contentsOpacity > 0 && firstCard.contentsOpacity < 1, "previous contents fade smoothly after the new card settles");
                tryCompare(firstCard, "contentsOpacity", 0, 1000);
                state.dismiss(overlay);
                wait(700);
                tryCompare(firstCard, "contentsOpacity", 1, 1000);
                const closeControl = findChild(firstCard, "notificationCloseButton");
                const timeLabel = findChild(firstCard, "notificationReceivedTime");
                mouseMove(surface.contentItem, 0, 0);
                wait(150);
                equal(closeControl.opacity, 0, "close button is hidden without hover");
                equal(timeLabel.opacity, 1, "received time is visible without hover");
                const newer = notificationFactory.createObject(surface, {id: 2, desktopEntry: "", appName: "Other app", summary: "A newer notification"});
                state.accept(newer);
                wait(60);
                equal(state.visibleEntries[0].notification, newer, "newest notification is at the top");
                const targetY = firstCard.height + firstCard.parent.spacing;
                check(firstCard.y > 0 && firstCard.y < targetY, "older card slides down smoothly");
                equal(firstCard.opacity, 1, "existing cards stay opaque while shifting down");
                tryCompare(firstCard, "y", targetY, 1000);
                state.dismiss(newer);
                tryCompare(firstCard, "y", 0, 1000);
                // Move the pointer over body text on a card without a default action.
                mouseMove(firstCard, 100, 60);
                wait(50);
                check(state.visibleEntries[0].paused, "hover pauses expiry");
                tryCompare(closeControl, "opacity", 1, 500);
                tryCompare(timeLabel, "opacity", 0, 500);
                const close = findChild(firstCard, "notificationCloseButton");
                const closePosition = close.mapToItem(firstCard, 0, 0);
                equal(Math.round(closePosition.y), Math.round(firstCard.width - closePosition.x - close.width), "close button has equal top and right insets");
                const restingColour = String(close.color);
                mouseMove(close, 16, 16);
                wait(150);
                const hoveredColour = String(close.color);
                check(hoveredColour !== restingColour, "close button has a hover state");
                mousePress(close, 16, 16);
                wait(150);
                check(String(close.color) !== hoveredColour, "close button has a pressed state");
                mouseMove(surface.contentItem, 0, 0);
                mouseRelease(surface.contentItem, 0, 0);
                mouseMove(close, 16, 16);
                wait(30);
                check(state.visibleEntries[0].paused, "hover over dismiss also pauses expiry");
                mouseMove(surface.contentItem, 0, 0);
                wait(30);
                check(!state.visibleEntries[0].paused, "leaving resumes expiry");
                const pressPoint = firstCard.mapToItem(surface.contentItem, 100, 60);
                mousePress(surface.contentItem, pressPoint.x, pressPoint.y);
                let previousOffset = firstCard.dragOffset;
                for (let distance = 2; distance <= 30; distance += 2) {
                    mouseMove(surface.contentItem, pressPoint.x + distance, pressPoint.y, 16);
                    const step = firstCard.dragOffset - previousOffset;
                    check(step <= 3, "drag starts without a jump: pointer step 2, card step " + step);
                    previousOffset = firstCard.dragOffset;
                }
                mouseRelease(surface.contentItem, pressPoint.x + 30, pressPoint.y);
                tryCompare(firstCard, "dragOffset", 0, 1000);
                mousePress(firstCard, 100, 60);
                mouseMove(firstCard, 110, 60, 20);
                mouseMove(firstCard, 130, 60, 20);
                wait(30);
                check(firstCard.dragOffset > 0);
                mouseRelease(firstCard, 130, 60);
                wait(50);
                check(firstCard.dragOffset > 0, "snap-back should still be animating");
                tryCompare(firstCard, "dragOffset", 0, 1000);
                equal(firstCard.dragOffset, 0);
                equal(state.visibleEntries.length, 1);
                state.resetExpiry(notification);
                equal(card(), firstCard, "updates preserve the existing delegate");
                mousePress(firstCard, 80, 60);
                mouseMove(firstCard, 100, 60, 20);
                mouseMove(firstCard, 160, 60, 20);
                mouseRelease(firstCard, 160, 60);
                wait(35);
                check(firstCard.dismissing, "dismissal is in flight");
                // Grab the remaining visible portion and reverse direction.
                mousePress(firstCard, 80, 60);
                check(!firstCard.dismissing, "press interrupts immediately at offset " + firstCard.slideOffset + " + " + firstCard.dragOffset);
                const heldOffset = firstCard.dragOffset;
                wait(480);
                equal(state.visibleEntries.length, 1, "holding cancels the pending close timer");
                check(!firstCard.dismissing, "grabbing interrupts dismissal");
                equal(firstCard.dragOffset, heldOffset, "card stays under the pointer");
                mouseMove(firstCard, 60, 60, 20);
                mouseMove(firstCard, 20, 60, 20);
                mouseRelease(firstCard, 20, 60);
                check(!firstCard.dismissing, "reverse release returns the card: " + firstCard.dragDelta);
                tryCompare(firstCard, "dragOffset", 0, 1000);
                equal(state.visibleEntries.length, 1, "dragging back cancels dismissal");
                mousePress(firstCard, 80, 60);
                mouseMove(firstCard, 100, 60, 20);
                mouseMove(firstCard, 160, 60, 20);
                mouseRelease(firstCard, 160, 60);
                tryVerify(() => state.visibleEntries.length === 0, 1000);
                equal(state.visibleEntries.length, 0);
                tryVerify(() => card() === null, 1000);
                const timed = notificationFactory.createObject(surface, {id: 3, expireTimeout: 0.8});
                state.accept(timed);
                tryCompare(card(), "slideOffset", 0, 1000);
                mouseMove(surface.contentItem, 0, 0);
                const expiringCard = card();
                tryVerify(() => state.visibleEntries.length === 0, 2000);
                check(expiringCard.retiring && surface.visible, "last expired card remains visible during its exit");
                wait(60);
                check(expiringCard.slideOffset > 0, "expired card slides to the right");
                equal(expiringCard.opacity, 1, "expired card never fades");
                tryVerify(() => card() === null, 1000);
                check(surface.mask.height === 0, "archived notification surface releases pointer input after the slide finishes");
                mouseClick(spawnButton, spawnButton.width / 2, spawnButton.height / 2);
                tryVerify(() => state.visibleEntries.length === 1, 2000);
                equal(state.visibleEntries[0].summary, "Notification preview", "button sends a real notification");
                tryCompare(card(), "slideOffset", 0, 1000);
                const ring = findChild(card(), "notificationExpiryProgress");
                check(ring.visible && ring.progress > 0 && ring.progress < 1, "timeout ring fills while notification is open");
                mouseMove(card(), 100, 60);
                wait(60);
                const progressBeforeHover = ring.progress;
                wait(300);
                equal(ring.progress, progressBeforeHover, "timeout ring pauses during hover");
                mouseMove(surface.contentItem, 0, 0);
                wait(150);
                check(ring.progress > progressBeforeHover, "timeout ring resumes after hover");
                state.dismiss(state.visibleEntries[0].notification);
                tryVerify(() => card() === null, 1000);
                for (let index = 0; index < 20; index += 1)
                    state.accept(notificationFactory.createObject(surface, {id: 100 + index, summary: "Stack item " + index}));
                wait(500);
                equal(state.visibleEntries.length, 20, "more than three notifications remain open");
                const stack = card();
                check(stack.collapsedStack, "same app collapses into a visual stack");
                equal(stack.groupCount, 20, "stack includes every notification");
                const stackedCards = Array.from(stack.parent.children).filter(item => item.objectName === "notificationCard");
                equal(stackedCards.filter(item => item.visible).length, 4, "large collapsed groups render only four layers");
                equal(surface.viewport.stackDepth(19), surface.viewport.stackDepth(3), "hidden notifications add no stack depth or spacing");
                check(stackedCards.filter(item => item.groupDepth >= 4).every(item => !item.visible), "extra cards and their shadows stay hidden");
                equal(findChild(surface.contentItem, "notificationGroupBackground").opacity, 0, "collapsed stack has no raised background");
                tryCompare(stack, "slideOffset", 0, 1000);
                const actualStackCard = Array.from(stack.parent.children).find(item => item.objectName === "notificationCard" && item.groupDepth === 1);
                tryCompare(actualStackCard, "y", stack.y + 8, 1000);
                equal(actualStackCard.layoutInset, 6, "real second card occupies the first inset stack layer");
                equal(actualStackCard.height, stack.height, "real collapsed cards align their bottom edges exactly");
                tryCompare(actualStackCard, "contentsOpacity", 0, 1000);
                equal(actualStackCard.contentsOpacity, 0, "collapsed backing cards hide their text and icon");
                equal(stack.contentsOpacity, 1, "top card contents remain visible");
                check(actualStackCard.visible && !actualStackCard.enabled, "real collapsed cards stay rendered without intercepting input");
                const dimOpacity = actualStackCard.opacity;
                const tuckedWidth = actualStackCard.width;
                const tuckedY = actualStackCard.y;
                const tuckedHeight = actualStackCard.height;
                mousePress(stack, 100, 60);
                mouseMove(stack, 110, 60, 20);
                mouseMove(stack, 135, 60, 20);
                equal(actualStackCard.height, tuckedHeight, "next stack head keeps its header height while multiple notifications remain");
                const upcomingToggle = findChild(actualStackCard, "notificationGroupToggle");
                check(upcomingToggle.visible, "next stack head reveals its Show more control");
                check(actualStackCard.countProgress > 0 && actualStackCard.countProgress < 1, "count transitions progressively with the drag");
                equal(state.visibleEntries.length, 20, "animated count does not remove a notification prematurely");
                check(actualStackCard.width > tuckedWidth && actualStackCard.width <= stack.width, "backing card grows towards the front card width during drag");
                check(actualStackCard.y < tuckedY && actualStackCard.y >= stack.y, "backing card rises out of its inset during drag");
                check(actualStackCard.opacity > dimOpacity, "dragging the head brightens the card beneath it");
                check(actualStackCard.contentsOpacity > 0 && actualStackCard.contentsOpacity < 1, "backing contents reveal progressively with the drag: " + actualStackCard.contentsOpacity + " / " + actualStackCard.revealProgress);
                mouseRelease(stack, 135, 60);
                tryVerify(() => stack.dragOffset === 0, 1000);
                tryCompare(actualStackCard, "contentsOpacity", 0, 1000);
                equal(actualStackCard.opacity, dimOpacity, "snap-back restores the backing card opacity");
                equal(actualStackCard.width, tuckedWidth, "snap-back restores the tucked card width");
                equal(actualStackCard.y, tuckedY, "snap-back restores the tucked card position");
                equal(actualStackCard.height, tuckedHeight, "snap-back restores aligned stack heights");
                tryVerify(() => stack.entranceComplete && stack.slideOffset === 0 && stack.dragOffset === 0, 1000);
                wait(30);
                mouseClick(stack, 100, 60);
                wait(100);
                check(stack.groupExpanded, "stack click expands after the drag has fully settled: " + stack.interruptedMotion);
                const revealingCard = Array.from(stack.parent.children).find(item => item.objectName === "notificationCard" && item.groupDepth === 1);
                check(revealingCard.y > stack.y + 8 && revealingCard.y < stack.y + stack.height + stack.parent.spacing, "expansion moves the card out from behind the head");
                equal(revealingCard, actualStackCard, "the same notification delegate moves out of the stack");
                equal(revealingCard.entranceOpacity, 1, "expanding cards move without fading in");
                wait(500);
                const groupCards = Array.from(stack.parent.children).filter(item => item.objectName === "notificationCard" && item.visible);
                equal(groupCards.length, 20, "expanding still exposes every notification");
                const secondCard = groupCards.find(item => item.groupDepth === 1);
                const thirdCard = groupCards.find(item => item.groupDepth === 2);
                equal(secondCard.contentsOpacity, 1, "expanded cards restore their contents");
                equal(secondCard.opacity, 1, "expanded notifications are fully opaque");
                equal(thirdCard.opacity, 1, "all expanded notifications have full opacity");
                equal(secondCard.color.a, 1, "expanded card backgrounds are also opaque");
                check(stack.groupExpanded, "click expands the app stack: visible=" + stack.visible + " interrupted=" + stack.interruptedMotion + " slide=" + stack.slideOffset);
                const viewport = findChild(surface.contentItem, "notificationViewport");
                check(viewport.contentHeight > viewport.height, "overflow is contained in a scrollable viewport");
                check(viewport.layer.enabled, "bottom mask is enabled while more notifications remain below");
                if (Quickshell.env("BINGUX_NOTIFICATION_STACK_SCREENSHOT")) {
                    const position = viewport.mapToGlobal(0, 0);
                    Quickshell.execDetached(["grim", "-g", Math.round(position.x) + "," + Math.round(position.y) + " " + Math.round(viewport.width) + "x" + Math.round(viewport.height), Quickshell.env("BINGUX_NOTIFICATION_STACK_SCREENSHOT")]);
                    wait(300);
                }
                mouseWheel(viewport, viewport.width / 2, viewport.height / 2, 0, -120, Qt.NoButton, Qt.NoModifier, 20);
                tryVerify(() => viewport.contentY > 0, 1000);
                equal(state.visibleEntries.length, 20, "scrolling keeps all notifications open");
                viewport.contentY = viewport.contentHeight - viewport.height;
                check(!viewport.layer.enabled, "last notification is fully readable at the end");
                viewport.contentY = 0;
                const toggle = findChild(stack, "notificationGroupToggle");
                const expandedY = secondCard.y;
                mouseClick(toggle, 40, 8);
                wait(100);
                check(secondCard.visible && secondCard.y < expandedY && secondCard.y > stack.y, "collapse slides the card back behind the head");
                wait(500);
                check(stack.collapsedStack, "show less collapses the group again");
                equal(stackedCards.filter(item => item.visible).length, 4, "collapsing restores the four-layer limit");
                tryCompare(actualStackCard, "y", stack.y + 8, 1000);
                equal(actualStackCard.height, stack.height, "collapse restores the exact stack geometry");
                equal(actualStackCard.contentsOpacity, 0, "collapse fades backing contents away again");
                equal(actualStackCard, secondCard, "collapse retains the real notification card");
                // Reverse expansion before it finishes: positions must remain continuous.
                mouseClick(toggle, 40, 8);
                wait(100);
                const intermediateY = actualStackCard.y;
                mouseClick(toggle, 40, 8);
                check(Math.abs(actualStackCard.y - intermediateY) < 5, "reversing the animation does not teleport cards");
                wait(500);
                tryCompare(actualStackCard, "y", stack.y + 8, 1000);
                equal(state.visibleEntries.length, 20, "collapse does not dismiss notifications");
                mouseClick(toggle, 40, 8);
                wait(350);
                const background = findChild(surface.contentItem, "notificationGroupBackground");
                tryCompare(background, "opacity", 0.55, 1000);
                equal(background.opacity, 0.55, "expanded group background is fully shown");
                state.dismiss(stack.notification);
                wait(60);
                equal(findChild(surface.contentItem, "notificationGroupBackground"), background, "removing the head retains the same group background");
                equal(background.opacity, 0.55, "background does not fade during dismissal");
                wait(500);
                equal(findChild(surface.contentItem, "notificationGroupBackground"), background, "background survives destruction of the old head");
                while (state.visibleEntries.length > 1) state.dismiss(state.visibleEntries[0].notification);
                wait(600);
                equal(background.opacity, 0, "background fades away when only one notification remains");
                const lastCard = card();
                equal(lastCard.layoutInset, 0, "last notification loses the group side padding");
                equal(lastCard.y, 0, "last notification loses the group top padding");
                state.dismiss(state.visibleEntries[0].notification);
                wait(700);
                check(findChild(surface.contentItem, "notificationGroupBackground") === null, "background is removed after the group empties");
                state.accept(notificationFactory.createObject(surface, {id: 901}));
                state.accept(notificationFactory.createObject(surface, {id: 902}));
                wait(800);
                const outgoing = Array.from(card().parent.children).find(item => item.objectName === "notificationCard" && item.notificationId === 902);
                const promoted = Array.from(outgoing.parent.children).find(item => item.objectName === "notificationCard" && item.notificationId === 901);
                if (outgoing.groupExpanded) surface.toggleGroup(outgoing.groupKey);
                wait(400);
                outgoing.dismissAnimated();
                wait(200);
                equal(promoted.contentsOpacity, 1, "next card is revealed before handover");
                equal(promoted.height, promoted.naturalHeight, "last remaining card sheds the group header space");
                check(!findChild(promoted, "notificationGroupToggle").visible, "standalone successor has no group control");
                let minimumContents = 1;
                let minimumWidth = promoted.width;
                const revealedWidth = promoted.width;
                for (let frame = 0; frame < 45; frame++) {
                    wait(16);
                    minimumContents = Math.min(minimumContents, promoted.contentsOpacity);
                    minimumWidth = Math.min(minimumWidth, promoted.width);
                }
                equal(minimumContents, 1, "revealed contents never blink when promoted to the top");
                equal(minimumWidth, revealedWidth, "revealed geometry never resets on promotion");
                state.dismiss(promoted.notification);
                wait(500);
                for (let index = 0; index < 4; index++)
                    state.accept(notificationFactory.createObject(surface, {id: 950 + index, summary: "Four-card stack " + index}));
                wait(800);
                const fourCards = Array.from(card().parent.children).filter(item => item.objectName === "notificationCard");
                const topOfFour = fourCards.find(item => item.groupHead);
                const nextOfFour = fourCards.find(item => item.groupDepth === 1);
                const deepest = fourCards.find(item => item.groupDepth === 3);
                const deepGeometry = [deepest.x, deepest.y, deepest.width, deepest.height, deepest.opacity].join(",");
                mousePress(topOfFour, 100, 60);
                mouseMove(topOfFour, 110, 60, 20);
                mouseMove(topOfFour, 135, 60, 20);
                equal(state.visibleEntries.length, 4, "four-card drag retains all notifications");
                equal(topOfFour.countProgress, 0, "outgoing header does not animate its count");
                equal(topOfFour.displayedGroupCount, 4, "outgoing card keeps its original count while dragged");
                check(nextOfFour.countProgress > 0 && nextOfFour.countProgress < 1, "four-to-three count transition follows drag progress");
                equal(nextOfFour.displayedGroupCount, 4 - (nextOfFour.countProgress >= 0.5 ? 1 : 0), "displayed count follows the animated digits");
                const previousDeep = deepGeometry.split(",").map(Number);
                check(deepest.x < previousDeep[0] && deepest.y < previousDeep[1] && deepest.width > previousDeep[2], "deepest card advances towards the preceding stack slot during drag");
                check(deepest.opacity > previousDeep[4], "deeper cards brighten towards the preceding depth together");
                equal(deepest.contentsOpacity, 0, "deeper cards keep contents hidden until they reach the front");
                mouseRelease(topOfFour, 135, 60);
                tryCompare(topOfFour, "dragOffset", 0, 1000);
                equal(nextOfFour.displayedGroupCount, 4, "cancelled drag retains the four-card count");
                equal([deepest.x, deepest.y, deepest.width, deepest.height, deepest.opacity].join(","), deepGeometry, "cancelling restores the entire backing stack");
                topOfFour.dismissAnimated();
                wait(700);
                equal(state.visibleEntries.length, 3, "dismissal removes exactly one notification");
                equal(nextOfFour.displayedGroupCount, 3, "count settles at three after dismissal completes");
            }
            function cleanupTestCase() { results.setText(checks + "FAILURES " + qtest_results.failCount); Qt.quit(); }
        }
    }
}
