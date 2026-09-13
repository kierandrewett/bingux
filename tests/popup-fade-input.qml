import QtQuick
import QtTest
import Quickshell
import "../shell/bingux"

ShellRoot {
    id: root
    property int clicks: 0
    FloatingWindow {
        id: host
        implicitWidth: 600
        implicitHeight: 400
    }
    ShellPopup {
        id: popup
        popupWidth: 280
        popupHeight: 120
        Rectangle {
            id: swatch
            x: 220
            y: 64
            width: 16
            height: 16
            color: "white"
        }
        ActionButton {
            id: button
            text: "Panel input test"
            onClicked: root.clicks++
        }
    }
    TestCase {
        name: "PopupFadeInput"
        when: host.visible
        function test_grouped_input() {
            for (const mode of ["native", "inline", "shadowed"]) {
                popup.hostItem = mode === "native" ? null : host.contentItem;
                popup.windowShadow = mode === "shadowed";
                popup.visible = true;
                wait(250);
                let frame = null;
                verify(popup.body.parent.grabToImage(result => frame = result));
                tryVerify(() => frame !== null);
                verify(frame.saveToFile(Quickshell.env("XDG_CONFIG_HOME") + "/popup-" + mode + ".png"));
                const point = swatch.mapToItem(popup.body.parent, 8, 8);
                console.log("SWATCH " + JSON.stringify({
                    mode: mode,
                    x: Math.round(point.x),
                    y: Math.round(point.y)
                }));
                const previous = root.clicks;
                mouseClick(button, button.width / 2, button.height / 2);
                compare(root.clicks, previous + 1, mode + " panel button accepts input");
                popup.presentationOpacity = .5;
                wait(50);
                mouseClick(button, button.width / 2, button.height / 2);
                compare(root.clicks, previous + 2, mode + " grouped fade accepts input");
                popup.visible = false;
                wait(200);
            }
            console.log("PASS: native, inline and shadowed popup buttons work during fades");
            Quickshell.quit();
        }
    }
}
