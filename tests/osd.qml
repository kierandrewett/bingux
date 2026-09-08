import QtQuick
import QtTest
import Quickshell

ShellRoot {
    OsdState { id: state }
    OsdSurface { id: surface; state: state }
    TestCase {
        name: "Osd"
        when: surface.testWindows.length > 0
        function check(value, message) { console.warn(message + ": " + value); verify(value, message); }
        function show(icon, level, maxLevel, label) {
            check(state.acceptRecord({protocolVersion: 2, type: "osd", monitorIndex: 0,
                outputNames: [Quickshell.screens[0].name], icon: icon, label: label || "",
                level: level, maxLevel: maxLevel}), "Accept OSD request");
        }
        function test_surface() {
            const window = surface.testWindows[0];
            const card = window.testCard;
            show("audio-volume-high-symbolic", .65, 1, "Headphones");
            tryCompare(card, "opacity", 1);
            tryCompare(card, "scale", 1);
            waitForRendering(card);
            check(window.visible && !window.focusable && window.testMask.width === 0 && window.testMask.height === 0,
                "OSD shows without taking keyboard or pointer input");
            check(findChild(card, "osdTitle").text === "Volume" && findChild(card, "osdDetail").text === "Headphones", "Control and device labels have separate roles");
            check(window.percentLabel === "65%", "Volume percentage is correct");
            const percentage = findChild(card, "osdPercent");
            tryCompare(percentage, "displayedValue", 65);
            check(Math.abs(card.x + card.width / 2 - window.width / 2) < 1, "Without sidebar OSD is centred on the output");
            surface.sidebarScreen = Quickshell.screens[0];
            surface.leftInset = 300;
            wait(30);
            check(Math.abs(card.x + card.width / 2 - (window.width + 300) / 2) < 1, "Left sidebar centres OSD in the remaining desktop");
            surface.leftInset = 0;
            surface.rightInset = 240;
            wait(30);
            check(Math.abs(card.x + card.width / 2 - (window.width - 240) / 2) < 1, "Right sidebar centres OSD in the remaining desktop");
            surface.sidebarScreen = {name: "another-output"};
            tryVerify(() => Math.abs(card.x + card.width / 2 - window.width / 2) < 1);
            check(Math.abs(card.x + card.width / 2 - window.width / 2) < 1, "A sidebar on another output does not shift this OSD");
            surface.sidebarScreen = Quickshell.screens[0];
            surface.rightInset = 0;
            const bar = findChild(card, "osdLevel");
            show("audio-volume-overamplified-symbolic", 1.25, 1.5, "Headphones");
            if (!Theme.reducedMotion) {
                wait(40);
                check(percentage.animating && percentage.progress > 0 && percentage.progress < 1 && percentage.direction === 1,
                    "Increasing percentages slide upwards");
                check(bar.width > bar.parent.width * .65 && bar.width < bar.parent.width * 1.25 / 1.5,
                    "Level changes ease between values");
            }
            wait(160);
            check(window.percentLabel === "125%", "Amplified volume is not incorrectly labelled as a percentage of the maximum");
            tryCompare(percentage, "displayedValue", 125);
            check(percentage.label(percentage.displayedValue) === "125%", "Sliding percentages support values above 99");
            show("audio-volume-muted-symbolic", 0, 1, "Headphones");
            if (!Theme.reducedMotion) {
                wait(35);
                check(percentage.animating && percentage.direction === -1, "Decreasing percentages slide downwards");
            }
            check(findChild(card, "osdDetail").text === "Muted", "Mute is clearly labelled");
            show("display-brightness-symbolic", .72, 0, "");
            check(window.hasLevel && window.percentLabel === "72%", "Brightness uses the default maximum when omitted by the compositor");
            wait(180);
            tryCompare(percentage, "displayedValue", 72);
            card.grabToImage(result => result.saveToFile("/tmp/bingux-notification-polish/osd-brightness.png"));
            wait(50);
            state.requests = {};
            if (!Theme.reducedMotion) {
                wait(45);
                check(window.visible && card.opacity > 0 && card.opacity < 1 && card.scale === 1,
                    "Close retains the surface and fades without scaling");
                check(findChild(card, "osdTitle").text === "Brightness", "Content remains present through the fade");
            }
            show("keyboard-brightness-symbolic", .4, 1, "");
            tryCompare(card, "opacity", 1);
            check(findChild(card, "osdTitle").text === "Keyboard brightness", "A new request reverses a close without blanking");
            show("microphone-sensitivity-muted-symbolic", 0, 1, "");
            check(findChild(card, "osdTitle").text === "Microphone" && findChild(card, "osdDetail").text === "Muted", "Microphone mute is supported");
            show("dialog-information-symbolic", -1, -1, "Touchpad disabled");
            check(!findChild(card, "osdTrack").visible, "Status-only requests hide the level bar");
            show("action-unavailable-symbolic", -1, -1, "");
            wait(50);
            check(window.iconOnly && card.width === card.height && card.width === Theme.osdIconTileSize + Theme.osdPadding * 2,
                "Unavailable actions use a compact square OSD");
            check(!findChild(card, "osdTitle").visible && !findChild(card, "osdDetail").visible
                && !findChild(card, "osdPercent").visible && !findChild(card, "osdTrack").visible,
                "Unavailable actions show only their icon");
            card.grabToImage(result => result.saveToFile("/tmp/bingux-notification-polish/osd-unavailable.png"));
            wait(50);
            show("audio-volume-high-symbolic", .65, 1, "Headphones");
            wait(50);
            check(!window.iconOnly && card.width === Theme.osdWidth && findChild(card, "osdTitle").visible,
                "Volume restores the full OSD after an unavailable action");
            tryVerify(() => !window.visible, 2200);
            check(card.opacity === 0, "OSD expires and fully unmaps");
            console.info("OSD_TEST_PASSED");
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
