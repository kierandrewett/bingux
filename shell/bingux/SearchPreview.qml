import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell

Item {
    id: root
    property string path: ""
    property string title: ""
    readonly property var helperCommand: Quickshell.env("BINGUX_PREVIEW_HELPER")
        ? [Quickshell.env("BINGUX_PREVIEW_HELPER")]
        : ["python3", decodeURIComponent(Qt.resolvedUrl("preview-document.py").toString().replace(/^file:\/\//, ""))]
    function zoomBy(steps) {
        if (document.item && document.item.details && !switching)
            document.item.setZoom(document.item.targetZoom * Math.pow(1.1, steps));
    }
    property bool showInformation: true
    property bool lightBackground: Theme.searchSurface.r * 0.2126 + Theme.searchSurface.g * 0.7152 + Theme.searchSurface.b * 0.0722 > 0.5
    property string displayedPath: ""
    property string displayedTitle: ""
    property bool switching: false
    function resetZoom() {
        if (document.item && !switching) document.item.setZoom(1);
    }
    onPathChanged: {
        if (!path) { reload.stop(); document.active = false; displayedPath = ""; switching = false; return; }
        switching = document.active;
        reload.restart();
    }
    Timer {
        id: reload
        interval: root.switching ? Theme.previewCloseMotion : 0
        onTriggered: {
            document.active = false;
            root.displayedPath = root.path;
            root.displayedTitle = root.title;
            root.switching = false;
            document.active = root.path !== "";
        }
    }
    Loader {
        id: document
        anchors.fill: parent
        active: false
        opacity: root.switching ? 0 : 1
        Behavior on opacity { NumberAnimation { duration: Theme.previewCloseMotion; easing.type: Easing.OutCubic } }
        enabled: !root.switching
        sourceComponent: Item {
            id: body
            objectName: "searchPreviewBody"
            property var details: null
            property string error: ""
            property real targetZoom: 1
            property real zoom: targetZoom
            property real zoomFocusX: 0
            property real zoomFocusY: 0
            Behavior on zoom {
                NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic }
            }
            onZoomChanged: Qt.callLater(() => {
                const view = body.textDocument ? textView : pages;
                view.contentX = Math.max(0, Math.min(zoomFocusX * zoom - view.width / 2, view.contentWidth - view.width));
                view.contentY = Math.max(0, Math.min(zoomFocusY * zoom - view.height / 2, view.contentHeight - view.height));
            })
            property bool showInformation: root.showInformation
            onShowInformationChanged: root.showInformation = showInformation
            property real entrance: 0
            Component.onCompleted: entrance = 1
            opacity: entrance
            transform: Translate { x: (1 - body.entrance) * 10 }
            Behavior on entrance { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
            readonly property bool textDocument: details !== null && details.kind === "text"
            readonly property bool imageDocument: details !== null && (details.kind === "image" || details.kind === "animation")
            readonly property bool mediaDocument: details !== null && ["video", "audio", "animation"].includes(details.kind)
            readonly property bool databaseDocument: details !== null && details.kind === "database"
            property bool lightBackground: root.lightBackground
            onLightBackgroundChanged: root.lightBackground = lightBackground
            readonly property real pageWidth: {
                const available = Math.max(64, pages.width - 32);
                if (!imageDocument) return available * zoom;
                const size = details.pages[0];
                return Math.min(available, Math.max(32, pages.height - 32) * size.width / size.height) * zoom;
            }
            function fileSize(bytes) {
                if (bytes < 1024) return bytes + " B";
                if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KiB";
                return (bytes / (1024 * 1024)).toFixed(1) + " MiB";
            }
            readonly property var metadataRows: {
                if (!details || !details.file) return [];
                const rows = [];
                if (details.file.created) rows.push({label: "Created", value: Qt.formatDateTime(new Date(details.file.created), "dd MMM yyyy, HH:mm")});
                rows.push({label: "Modified", value: Qt.formatDateTime(new Date(details.file.modified), "dd MMM yyyy, HH:mm")});
                if (details.image && details.image.contentCreated) rows.push({label: "Content created", value: Qt.formatDateTime(new Date(details.image.contentCreated), "dd MMM yyyy, HH:mm")});
                if (imageDocument) rows.push({label: "Dimensions", value: details.pages[0].width + " × " + details.pages[0].height});
                else if (details.kind === "pdf") rows.push({label: "Pages", value: String(details.pages.length)});
                if (details.duration !== undefined) rows.push({label: "Duration", value: Math.floor(details.duration / 60) + ":" + String(Math.floor(details.duration % 60)).padStart(2, "0")});
                if (details.width) rows.push({label: "Dimensions", value: details.width + " × " + details.height});
                if (details.codec) rows.push({label: "Codec", value: details.codec});
                if (details.tables) rows.push({label: "Tables", value: String(details.tables.length)});
                if (details.image) {
                    if (details.image.resolution) rows.push({label: "Resolution", value: details.image.resolution});
                    if (details.image.colourSpace) rows.push({label: "Colour space", value: details.image.colourSpace});
                    if (details.image.colourProfile) rows.push({label: "Colour profile", value: details.image.colourProfile});
                }
                if (details.frontmatter) {
                    for (const key of Object.keys(details.frontmatter)) rows.push({label: key, value: String(details.frontmatter[key])});
                }
                if (details.frontmatterWarning) rows.push({label: "Frontmatter", value: details.frontmatterWarning});
                rows.push({label: "Where", value: root.displayedPath.substring(0, root.displayedPath.lastIndexOf("/")) || "/"});
                return rows;
            }
            function setZoom(value) {
                const view = textDocument ? textView : pages;
                view.cancelFlick();
                zoomFocusX = (view.contentX + view.width / 2) / zoom;
                zoomFocusY = (view.contentY + view.height / 2) / zoom;
                targetZoom = Math.max(0.5, Math.min(4, value));
            }

            DocumentPreviewRequest {
                command: root.helperCommand.concat(["info", root.displayedPath])
                onResponse: data => {
                    body.error = data.error || "";
                    body.details = data.error ? null : data;
                }
            }
            component PreviewButton: ActionButton {
                id: previewControl
                property real iconRotation: 0
                implicitWidth: 28
                implicitHeight: 28
                focusPolicy: Qt.TabFocus
                flat: true
                horizontalPadding: 4
                verticalPadding: 4
                scale: down ? 0.9 : 1
                Behavior on scale { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
                ShellTooltip { visible: previewControl.hovered; text: previewControl.Accessible.name }
                contentItem: Item {
                    SymbolicIcon {
                        objectName: "previewControlIcon"
                        anchors.centerIn: parent
                        width: Theme.iconSize
                        height: Theme.iconSize
                        rotation: previewControl.iconRotation
                        Behavior on rotation { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
                        source: Quickshell.iconPath(previewControl.iconName)
                        color: Theme.muted
                        opacity: previewControl.enabled ? 1 : 0.4
                    }
                }
            }
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 52
                    Layout.leftMargin: 14
                    Layout.rightMargin: 8
                    spacing: 10
                    OsIconImage {
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 28
                        source: "file-preview:" + encodeURIComponent(root.displayedPath)
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            Layout.fillWidth: true
                            text: root.displayedTitle
                            textFormat: Text.PlainText
                            elide: Text.ElideMiddle
                            color: Theme.text
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSize
                            font.weight: Font.DemiBold
                        }
                        Text {
                            objectName: "previewFileSummary"
                            Layout.fillWidth: true
                            text: body.details && body.details.file ? body.details.file.type + " · " + body.fileSize(body.details.file.sizeBytes) : "Preview"
                            elide: Text.ElideRight
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                        }
                    }
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.outline }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    color: body.lightBackground ? "#f4f4f4" : "#222222"
                    radius: 0
                    clip: true
                    Image {
                        objectName: "previewNoise"
                        anchors.fill: parent
                        source: Qt.resolvedUrl("preview-assets/imagedoc-darknoise.png")
                        fillMode: Image.Tile
                        opacity: body.lightBackground ? 0 : 1
                        Behavior on opacity { NumberAnimation { duration: Theme.previewMotion } }
                    }
                    Image {
                        anchors.fill: parent
                        source: Qt.resolvedUrl("preview-assets/imagedoc-lightnoise.png")
                        fillMode: Image.Tile
                        opacity: body.lightBackground ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.previewMotion } }
                    }
                    ListView {
                        id: pages
                        objectName: "searchPreviewPages"
                        anchors.fill: parent
                        anchors.margins: 8
                        visible: body.details !== null && !body.textDocument && !body.mediaDocument && !body.databaseDocument
                        model: visible ? body.details.pages : []
                        contentWidth: Math.max(width, body.pageWidth)
                        flickableDirection: Flickable.HorizontalAndVerticalFlick
                        boundsBehavior: Flickable.StopAtBounds
                        spacing: 12
                        cacheBuffer: Math.max(0, height)
                        ScrollBar.vertical: ScrollBar {}
                        HorizontalWheelScroll { viewport: pages }
                        ScrollBar.horizontal: ScrollBar {}
                        delegate: Item {
                            id: page
                            required property int index
                            required property var modelData
                            property string imageSource: ""
                            property string error: ""
                            readonly property real renderedHeight: body.pageWidth * modelData.height / modelData.width
                            width: Math.max(pages.width, body.pageWidth)
                            height: body.imageDocument ? Math.max(pages.height, renderedHeight) : renderedHeight
                            DocumentPreviewRequest {
                                command: root.helperCommand.concat(["page", root.displayedPath, String(page.index), String(Math.ceil(body.pageWidth * 2))])
                                active: page.y + page.height >= pages.contentY - pages.height && page.y <= pages.contentY + pages.height * 2
                                onResponse: data => { page.imageSource = data.image || ""; page.error = data.error || ""; }
                            }
                            Rectangle {
                                id: pageSurface
                                objectName: "previewPageSurface"
                                anchors.centerIn: parent
                                width: body.pageWidth
                                height: page.renderedHeight
                                color: body.details.kind === "pdf" && pageImage.revealed ? "white" : "transparent"
                                Image {
                                    id: pageImage
                                    anchors.fill: parent
                                    source: page.imageSource
                                    property bool revealed: false
                                    onStatusChanged: if (status === Image.Ready) revealed = true
                                    opacity: revealed ? 1 : 0
                                    Behavior on opacity { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
                                    asynchronous: true
                                    fillMode: Image.PreserveAspectFit
                                    cache: false
                                }
                                PreviewSpinner {
                                    anchors.centerIn: parent
                                    loading: !pageImage.revealed && !page.error
                                    colour: body.lightBackground ? "#555555" : Theme.muted
                                }
                                Text {
                                    anchors.centerIn: parent
                                    visible: page.error !== ""
                                    text: page.error
                                    color: body.details.kind === "pdf" ? "#454545" : Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: Theme.fontSmall
                                    width: Math.min(parent.width - 24, 280)
                                    wrapMode: Text.Wrap
                                    horizontalAlignment: Text.AlignHCenter
                                }
                            }
                        }
                    }
                    Flickable {
                        id: textView
                        anchors.fill: parent
                        anchors.margins: 16
                        visible: body.textDocument
                        opacity: visible ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
                        contentWidth: width
                        contentHeight: textContent.implicitHeight
                        boundsBehavior: Flickable.StopAtBounds
                        ScrollBar.vertical: ScrollBar {}
                        Text {
                            id: textContent
                            width: textView.width
                            text: body.textDocument ? (body.details.renderedText || body.details.text) : ""
                            textFormat: body.details && body.details.format !== "plain" ? Text.RichText : Text.PlainText
                            wrapMode: Text.Wrap
                            color: body.lightBackground ? "#222222" : Theme.text
                            font.family: body.details && body.details.format !== "plain" ? Theme.fontFamily : "monospace"
                            font.pixelSize: 13 * body.zoom
                        }
                    }
                    Loader {
                        id: mediaView
                        anchors.fill: parent
                        active: body.mediaDocument
                        source: active ? Qt.resolvedUrl("PreviewMedia.qml") : ""
                        onLoaded: { item.details = body.details; item.zoom = Qt.binding(() => body.zoom); }
                    }
                    Loader {
                        anchors.fill: parent
                        active: body.databaseDocument
                        sourceComponent: PreviewDatabase {
                            path: root.displayedPath
                            helperCommand: root.helperCommand
                            initialData: body.details
                            zoom: body.zoom
                        }
                    }
                    PreviewSpinner {
                        objectName: "previewLoadingSpinner"
                        anchors.centerIn: parent
                        loading: body.details === null && body.error === ""
                        colour: body.lightBackground ? "#555555" : Theme.muted
                    }
                    Text {
                        anchors.centerIn: parent
                        width: parent.width - 48
                        visible: body.error !== ""
                        text: body.error
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        horizontalAlignment: Text.AlignHCenter
                        color: body.lightBackground ? "#555555" : Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize
                    }
                }
                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.outline }
                RowLayout {
                    Layout.fillWidth: true
                    objectName: "previewToolbar"
                    Layout.preferredHeight: 32
                    Layout.leftMargin: 14
                    Layout.rightMargin: 8
                    spacing: 4
                    Text {
                        Layout.fillWidth: true
                        text: body.details && body.details.kind === "pdf"
                            ? (Math.max(0, pages.indexAt(pages.contentX + pages.width / 2, pages.contentY + pages.height / 2)) + 1) + " / " + body.details.pages.length
                            : body.textDocument && body.details.truncated ? "First 256 KiB" : "Preview"
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: 11
                    }
                    PreviewButton { objectName: "previewBackgroundToggle"; iconName: body.lightBackground ? "weather-clear-night-symbolic" : "weather-clear-symbolic"; Accessible.name: body.lightBackground ? "Use dark background" : "Use light background"; onClicked: body.lightBackground = !body.lightBackground; }
                    PreviewButton { iconName: "zoom-out-symbolic"; Accessible.name: "Zoom out"; enabled: body.details !== null && body.zoom > 0.5; onClicked: body.setZoom(body.targetZoom / 1.25); }
                    PreviewButton {
                        objectName: "previewZoomReset"
                        implicitWidth: 48
                        text: Math.round(body.zoom * 100) + "%"
                        Accessible.name: "Reset zoom to 100% (Ctrl+0)"
                        enabled: body.details !== null
                        onClicked: body.setZoom(1)
                        contentItem: Text {
                            text: parent.text
                            color: Theme.muted
                            opacity: parent.enabled ? 1 : 0.4
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                    PreviewButton { objectName: "previewZoomIn"; iconName: "zoom-in-symbolic"; Accessible.name: "Zoom in"; enabled: body.details !== null && body.zoom < 4; onClicked: body.setZoom(body.targetZoom * 1.25); }
                    PreviewButton { objectName: "previewFit"; iconName: "zoom-fit-best-symbolic"; Accessible.name: "Reset zoom (Ctrl+0)"; enabled: body.details !== null; onClicked: body.setZoom(1); }
                }
                Item {
                    id: facts
                    objectName: "previewFileMetadata"
                    property real reveal: 0
                    property real expansion: body.showInformation ? 1 : 0
                    property int rowCount: body.metadataRows.length
                    onRowCountChanged: if (rowCount > 0) reveal = 1
                    Behavior on reveal { NumberAnimation { duration: Theme.reducedMotion ? 0 : 260; easing.type: Easing.OutCubic } }
                    Behavior on expansion { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
                    Layout.fillWidth: true
                    Layout.leftMargin: 14
                    Layout.rightMargin: 14
                    Layout.preferredHeight: (36 + Math.min(rowCount * 21, 168) * expansion) * reveal
                    Layout.minimumHeight: 0
                    opacity: reveal
                    clip: true
                    ColumnLayout {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        spacing: 0
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 24
                            Text {
                                Layout.fillWidth: true
                                text: "Information"
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                font.weight: Font.DemiBold
                            }
                            PreviewButton {
                                objectName: "previewInformationToggle"
                                iconName: "pan-down-symbolic"
                                iconRotation: body.showInformation ? 180 : 0
                                Accessible.name: body.showInformation ? "Hide information" : "Show information"
                                onClicked: body.showInformation = !body.showInformation
                            }
                        }
                        Flickable {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Math.min(facts.rowCount * 21, 168) * facts.expansion
                            contentWidth: width
                            contentHeight: facts.rowCount * 21
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
                            ColumnLayout {
                                width: parent.width
                                spacing: 0
                        Repeater {
                            model: body.metadataRows
                            Item {
                                id: factRow
                                required property var modelData
                                required property int index
                                property bool entered: false
                                Layout.fillWidth: true
                                Layout.preferredHeight: 21
                                opacity: entered && body.showInformation ? 1 : 0
                                transform: Translate { y: (1 - factRow.opacity) * 5 }
                                Behavior on opacity { NumberAnimation { duration: Theme.previewMotion; easing.type: Easing.OutCubic } }
                                Timer { interval: Theme.reducedMotion ? 0 : Math.min(parent.index, 5) * 22; running: true; onTriggered: parent.entered = true }
                                RowLayout {
                                    anchors.fill: parent
                                    anchors.bottomMargin: 1
                                    spacing: 10
                                    Text {
                                        Layout.preferredWidth: 98
                                        text: modelData.label
                                        textFormat: Text.PlainText
                                        elide: Text.ElideRight
                                        color: Theme.muted
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 11
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        horizontalAlignment: Text.AlignRight
                                        text: modelData.value
                                        textFormat: Text.PlainText
                                        color: Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 11
                                        font.weight: Font.Medium
                                        elide: modelData.label === "Where" ? Text.ElideMiddle : Text.ElideRight
                                        Accessible.name: modelData.label + ": " + modelData.value
                                    }
                                }
                                Rectangle {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    visible: factRow.index < body.metadataRows.length - 1
                                    height: 1
                                    color: Qt.rgba(Theme.text.r, Theme.text.g, Theme.text.b, 0.08)
                                }
                            }
                        }
                            }
                        }
                    }
                }
            }
        }
    }
}
