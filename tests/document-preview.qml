import QtQuick
import QtTest
import Quickshell
import Quickshell.Wayland

SearchOverlay {
    id: preview
    visible: true
    WlrLayershell.namespace: "bingux-document-preview-test"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {}
    property int activationCount: 0
    function activateResult(result) { activationCount++; }
    function find(item, name) {
        if (item.objectName === name) return item;
        for (const child of item.children || []) {
            const found = find(child, name);
            if (found) return found;
        }
        return null;
    }
    TestCase {
        parent: preview.contentItem
        name: "DocumentPreview"
        when: preview.visible
        function equal(actual, expected, message) {
            console.warn(message + ': ' + actual + ' expected ' + expected);
            compare(actual, expected, message);
        }
        function test_preview() {
            wait(250);
            preview.displayedResults = [{ resultId: "pdf", providerId: "files", kind: "file", title: "Document preview", subtitle: Quickshell.env("BINGUX_PREVIEW_FIXTURES") + "/document.pdf", icon: "application-pdf" }, { resultId: "text", providerId: "files", kind: "file", title: "Text preview", subtitle: Quickshell.env("BINGUX_PREVIEW_FIXTURES") + "/note.txt", icon: "text-x-generic" }];
            preview.displayedResults = preview.displayedResults.concat([{resultId: "photo", providerId: "files", kind: "file", title: "Photo preview", subtitle: Quickshell.env("BINGUX_PREVIEW_FIXTURES") + "/photo.png", icon: "image-x-generic"}]);
            preview.selectedIndex = 0;
            wait(250);
            const surface = preview.find(preview.contentItem, "searchSurface");
            const initialX = surface.x;
            const initialHeight = surface.height;
            equal(preview.previewOpen, false, "highlight alone does not open preview");
            keyClick(Qt.Key_Right);
            const panel = preview.find(preview.contentItem, "searchPreviewSurface");
            if (!Theme.reducedMotion) equal(panel.reveal < 1, true, "preview starts with a slide and fade");
            wait(30);
            const loading = preview.find(preview.contentItem, "previewLoadingSpinner");
            equal(loading !== null && (loading.loading || preview.find(preview.contentItem, "searchPreviewBody").details !== null), true, "preview shows a spinner until prepared content is ready");
            tryVerify(() => preview.find(preview.contentItem, "searchPreviewBody").details !== null, 4000);
            const enteringFacts = preview.find(preview.contentItem, "previewFileMetadata");
            if (!Theme.reducedMotion) equal(enteringFacts.reveal < 1, true, "file facts animate when metadata arrives");
            wait(1200);
            const body = preview.find(preview.contentItem, "searchPreviewBody");
            equal(preview.previewOpen, true, "Right Arrow opens preview");
            const resultList = preview.find(preview.contentItem, "searchResultsList");
            const selectedRow = resultList.itemAtIndex(0);
            equal(preview.find(selectedRow, "searchPreviewLabel").text, "Hide", "open preview shows Hide");
            equal(preview.find(selectedRow, "searchSelectionChevron").rotation, 180, "open preview reverses the chevron");
            equal(preview.find(selectedRow, "searchSelectionChevron").x < preview.find(selectedRow, "searchPreviewLabel").x, true, "Hide text moves to the right of the reversed chevron");
            equal(preview.find(preview.contentItem, "searchPreviewSurface").height, 480, "preview uses compact height");
            equal(preview.find(preview.contentItem, "previewFileSummary").text.includes("KiB"), true, "file summary includes size");
            equal(body.metadataRows.some(row => row.label === "Modified"), true, "metadata includes modification date");
            equal(surface.width, 660, "search keeps its width");
            equal(surface.x, initialX, "opening preview does not move search");
            equal(surface.height, initialHeight, "opening preview does not stretch search");
            equal(body.details && body.details.kind, "pdf", "Right Arrow loads selected PDF");
            const pages = preview.find(preview.contentItem, "searchPreviewPages");
            equal(pages.count, 2, "PDF pages are available");
            tryVerify(() => pages.itemAtIndex(0).imageSource !== "", 4000);
            mouseClick(preview.find(preview.contentItem, "previewZoomIn"));
            if (!Theme.reducedMotion) equal(body.zoom < 1.25, true, "zoom starts smoothly instead of jumping");
            tryCompare(body, "zoom", 1.25, 1000);
            equal(pages.contentWidth > pages.width, true, "zoom provides horizontal pan space");
            const zoomReset = preview.find(preview.contentItem, "previewZoomReset");
            equal(zoomReset.text, "125%", "percentage button displays current zoom");
            mouseClick(zoomReset);
            tryCompare(body, "zoom", 1, 1000);
            equal(zoomReset.text, "100%", "clicking the percentage resets zoom");
            const zoomIcon = preview.find(preview.find(preview.contentItem, "previewZoomIn"), "previewControlIcon");
            equal(zoomIcon.width, Theme.iconSize, "zoom icons use the search icon size");
            mouseClick(preview.find(preview.contentItem, "previewFit"));
            tryCompare(body, "zoom", 1, 1000);
            wait(150);
            mouseMove(pages, pages.width / 2, pages.height / 2);
            mouseWheel(pages, pages.width / 2, pages.height / 2, 0, -360, Qt.NoButton);
            wait(150);
            equal(pages.contentY > 0, true, "mouse wheel scrolls through PDF pages");
            mouseClick(preview.find(preview.contentItem, "previewZoomIn"));
            mouseClick(preview.find(preview.contentItem, "previewZoomIn"));
            wait(250);
            const initialPanX = pages.contentX;
            mousePress(pages, pages.width * 0.75, pages.height / 2);
            mouseMove(pages, pages.width * 0.4, pages.height / 2, 100);
            mouseMove(pages, pages.width * 0.2, pages.height / 2, 100);
            mouseRelease(pages, pages.width * 0.2, pages.height / 2);
            wait(150);
            equal(pages.contentX > initialPanX, true, "drag pans the zoomed document");
            const previousZoom = body.zoom;
            const input = preview.find(preview.contentItem, "searchInput");
            mouseMove(input, 30, 20);
            mouseWheel(input, 30, 20, 0, 120, Qt.NoButton, Qt.ControlModifier);
            wait(100);
            equal(body.zoom > previousZoom, true, "Ctrl+wheel zooms from the search input without hovering over the preview");
            keyClick(Qt.Key_0, Qt.ControlModifier);
            tryCompare(body, "zoom", 1, 1000);
            equal(input.text, "", "Ctrl+0 resets without typing into search");
            const facts = preview.find(preview.contentItem, "previewFileMetadata");
            const fullFactsHeight = facts.height;
            mouseClick(preview.find(preview.contentItem, "previewInformationToggle"));
            if (!Theme.reducedMotion) equal(facts.height > 36, true, "facts collapse smoothly");
            wait(250);
            equal(facts.height < fullFactsHeight, true, "facts collapse reaches compact height");
            mouseClick(preview.find(preview.contentItem, "previewInformationToggle"));
            wait(250);
            equal(facts.height, fullFactsHeight, "facts expand back to their full height");
            pages.contentY = 0;
            wait(100);
            if (Quickshell.env("BINGUX_PREVIEW_SCREENSHOT"))
                preview.find(preview.contentItem, "searchPreviewSurface").grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_PREVIEW_SCREENSHOT")));
            wait(200);
            preview.selectedIndex = 1;
            wait(500);
            const textBody = preview.find(preview.contentItem, "searchPreviewBody");
            equal(textBody.details && textBody.details.kind, "text", "changing highlight replaces the preview");
            equal(textBody.details.text, "A plain text preview.\n", "text renders without markup interpretation");
            preview.selectedIndex = 2;
            wait(600);
            const photoBody = preview.find(preview.contentItem, "searchPreviewBody");
            const photoPages = preview.find(preview.contentItem, "searchPreviewPages");
            equal(photoBody.details.kind, "image", "photo preview loads");
            tryVerify(() => photoPages.itemAtIndex(0).imageSource !== "", 4000);
            const photo = preview.find(photoPages, "previewPageSurface");
            const photoPosition = photo.mapToItem(photoPages, photo.width / 2, photo.height / 2);
            equal(Math.abs(photoPosition.x - photoPages.width / 2) < 1, true, "photo is centred horizontally");
            equal(Math.abs(photoPosition.y - photoPages.height / 2) < 1, true, "photo is centred vertically");
            equal(photo.height <= photoPages.height && photo.width <= photoPages.width, true, "Fit keeps the full photo visible");
            equal(photoBody.metadataRows.some(row => row.label === "Dimensions" && row.value === "480 × 240"), true, "photo metadata includes dimensions");
            const noise = preview.find(preview.contentItem, "previewNoise");
            equal(noise.status, Image.Ready, "Firefox background texture loads");
            wait(250);
            if (Quickshell.env("BINGUX_PREVIEW_SCREENSHOT"))
                preview.find(preview.contentItem, "searchPreviewSurface").grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_PREVIEW_SCREENSHOT") + ".photo-dark.png"));
            wait(150);
            mouseClick(preview.find(preview.contentItem, "previewBackgroundToggle"));
            wait(100);
            equal(photoBody.lightBackground, true, "background switches to light");
            wait(250);
            if (Quickshell.env("BINGUX_PREVIEW_SCREENSHOT"))
                preview.find(preview.contentItem, "searchPreviewSurface").grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_PREVIEW_SCREENSHOT") + ".photo-light.png"));
            wait(150);
            const photoZoom = photoBody.zoom;
            mouseWheel(photoPages, photoPages.width / 2, photoPages.height / 2, 0, -120, Qt.NoButton, Qt.ControlModifier);
            wait(100);
            equal(photoBody.zoom < photoZoom, true, "Ctrl+scroll zooms photos out without a click");
            const zoomOutside = photoBody.targetZoom;
            mouseWheel(preview.contentItem, 10, 10, 0, 120, Qt.NoButton, Qt.ControlModifier);
            wait(250);
            equal(photoBody.zoom > zoomOutside, true, "Ctrl+scroll works outside both cards");
            const extraFiles = [
                {name: "notes.md", kind: "text", format: "markdown"},
                {name: "page.html", kind: "text", format: "html"},
                {name: "table.csv", kind: "text", format: "table"},
                {name: "message.eml", kind: "text", format: "email"},
                {name: "archive.zip", kind: "text", format: "archive"},
                {name: "data.sqlite", kind: "database"},
                {name: "animated.gif", kind: "animation"},
                {name: "clip.mp4", kind: "video"},
                {name: "office.docx", kind: "pdf"}
            ];
            for (const file of extraFiles) {
                preview.displayedResults = [{resultId: file.name, providerId: "files", kind: "file", title: file.name, subtitle: Quickshell.env("BINGUX_PREVIEW_FIXTURES") + "/" + file.name, icon: "text-x-generic"}];
                preview.selectedIndex = 0;
                wait(200);
                tryVerify(() => { const current = preview.find(preview.contentItem, "searchPreviewBody"); return current && current.details !== null && current.details.kind === file.kind; }, 12000);
                const current = preview.find(preview.contentItem, "searchPreviewBody");
                equal(current.lightBackground, true, "background preference survives file changes");
                if (file.format) equal(current.details.format, file.format, "formatted document is detected");
                if (file.kind === "database") {
                    tryVerify(() => preview.find(preview.contentItem, "previewDatabase") !== null, 1000);
                    const database = preview.find(preview.contentItem, "previewDatabase");
                    equal(database.tableData.rows[0][1], "First row", "database renders actual values");
                    database.load("other", 0);
                    tryVerify(() => database.tableData.table === "other", 3000);
                    equal(database.tableData.rows[0][0], "Second table", "database table selection loads rows");
                }
                if (file.kind === "animation") {
                    tryVerify(() => preview.find(preview.contentItem, "previewAnimatedImage") !== null, 2000);
                    const gif = preview.find(preview.contentItem, "previewAnimatedImage");
                    tryCompare(gif, "status", Image.Ready, 2000);
                    equal(gif.frameCount, 2, "GIF preserves animation frames");
                }
                if (file.kind === "video") {
                    tryVerify(() => preview.find(preview.contentItem, "previewMedia") !== null, 2000);
                    const media = preview.find(preview.contentItem, "previewMedia");
                    tryVerify(() => media.details.duration > 0, 2000);
                    if (!Theme.reducedMotion) tryVerify(() => media.playing, 2000);
                    const control = preview.find(preview.contentItem, "previewMediaPlayPause");
                    mouseClick(control);
                    if (!Theme.reducedMotion) equal(media.playing, false, "video controls pause playback");
                    else equal(media.playing, true, "video controls start playback with reduced motion");
                    if (!media.playing) media.togglePlayback();
                    mouseMove(preview.contentItem, 10, 10);
                    wait(50);
                    media.controlsAwake = false;
                    const transport = preview.find(preview.contentItem, "previewMediaTransport");
                    tryCompare(transport, "opacity", 0, 1000);
                    mouseMove(media, media.width / 2, media.height / 2);
                    tryCompare(transport, "opacity", 1, 1000);
                    media.togglePlayback();
                    wait(250);
                    if (Quickshell.env("BINGUX_PREVIEW_SCREENSHOT"))
                        panel.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_PREVIEW_SCREENSHOT") + ".video.png"));
                    wait(150);
                    control.forceActiveFocus();
                    current.setZoom(1.5);
                    keyClick(Qt.Key_0, Qt.ControlModifier);
                    equal(current.targetZoom, 1, "Ctrl+0 resets while a playback control has focus");
                    tryCompare(current, "zoom", 1, 1000);
                }
                wait(400);
                if (Quickshell.env("BINGUX_PREVIEW_SCREENSHOT") && file.kind !== "video")
                    panel.grabToImage(result => result.saveToFile(Quickshell.env("BINGUX_PREVIEW_SCREENSHOT") + "." + file.name + ".png"));
                wait(150);
            }
            input.forceActiveFocus();
            keyClick(Qt.Key_Left);
            if (!Theme.reducedMotion) equal(panel.visible, true, "preview remains visible for its exit animation");
            wait(250);
            equal(preview.previewOpen, false, "Left Arrow closes preview");
            equal(preview.visible, true, "closing preview keeps search open");
            keyClick(Qt.Key_Right);
            wait(150);
            keyClick(Qt.Key_Escape);
            equal(preview.closing, true, "Escape closes search, including its preview");
            preview.visible = false;
            wait(50);
            equal(preview.find(preview.contentItem, "searchPreviewBody"), null, "closing unloads preview and pending work");
            console.warn("DOCUMENT_PREVIEW_PASS");
        }
        function cleanupTestCase() { Qt.quit(); }
    }
}
