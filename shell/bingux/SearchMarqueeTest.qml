import QtQuick
import Quickshell
import Quickshell.Wayland

// Native QML timing regression: qs -p shell/bingux/SearchMarqueeTest.qml

PanelWindow {
    id: preview
    visible: true
    implicitWidth: 440
    implicitHeight: 70
    anchors {
        top: true
        left: true
    }
    margins.top: 100
    margins.left: 100
    color: Theme.searchSurface
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    function find(item, name) {
        if (item.objectName === name)
            return item;
        for (const child of item.children || []) {
            const found = find(child, name);
            if (found)
                return found;
        }
        return null;
    }
    function check(condition, message) {
        if (!condition) {
            console.error("FAIL: " + message);
            Qt.exit(1);
        }
        console.log("PASS: " + message);
    }
    SearchResult {
        id: row
        width: parent.width
        y: 10
        selected: true
        query: "report"
        result: ({
                providerId: "files",
                kind: "file",
                title: "A very long report title with enough words to overflow the entire result row.pdf",
                subtitle: "/home/example/Documents/research/long-project-name/archive/analysis/final-report-with-all-supporting-material.pdf",
                icon: "application-pdf"
            })
    }
    SearchLaunchEffect {
        id: burst
    }
    Timer {
        interval: 300
        running: true
        onTriggered: {
            preview.check(preview.find(row, "searchPathMarquee").offset === 0, "no scrolling before 500ms");
            preview.check(preview.find(row, "searchSelectionChevron").opacity === 1, "chevron finishes revealing");
            const replacement = Qt.createComponent("SearchResult.qml").createObject(preview.contentItem, {
                result: row.result,
                query: row.query,
                selected: true,
                visible: false,
                claimChevronAnimation: () => false
            });
            preview.check(replacement !== null, "replacement delegate created");
            preview.check(preview.find(replacement, "searchSelectionChevron").opacity === 1, "typing replacement does not replay entrance");
            replacement.destroy();
            burst.play(row.activationIcon, row.activationIconSource, row.symbolic);
            preview.check(Theme.reducedMotion ? !burst.running : burst.running && burst.scale === 1 && burst.opacity === 1, "launch starts at original size and opacity, or skips reduced motion");
        }
    }
    Timer {
        interval: 400
        running: true
        onTriggered: {
            preview.check(row.activationIcon.scale === 1 && row.activationIcon.opacity === 1, "launch does not change original icon");
            preview.check(Theme.reducedMotion || (burst.scale > 1 && burst.scale < 4 && burst.opacity > 0 && burst.opacity < 1), "independent icon grows and fades");
        }
    }
    Timer {
        interval: 650
        running: true
        onTriggered: preview.check(!burst.running && (Theme.reducedMotion || (burst.scale === 4 && burst.opacity === 0)), "launch completes at four times size and zero opacity")
    }
    Timer {
        interval: 1200
        running: true
        onTriggered: {
            const path = preview.find(row, "searchPathMarquee");
            const title = preview.find(row, "searchTitleMarquee");
            preview.check(Theme.reducedMotion ? path.offset === 0 : path.offset > 0, "path motion respects selection delay and reduced motion");
            preview.check(Theme.reducedMotion ? title.offset === 0 : title.offset > 0, "title motion respects selection delay and reduced motion");
            row.selected = false;
        }
    }
    Timer {
        interval: 1500
        running: true
        onTriggered: {
            preview.check(preview.find(row, "searchPathMarquee").offset === 0, "deselect resets path");
            preview.check(preview.find(row, "searchTitleMarquee").offset === 0, "deselect resets title");
            preview.check(preview.find(row, "searchSelectionChevron").opacity === 0, "deselect hides chevron");
            Qt.quit();
        }
    }
}
