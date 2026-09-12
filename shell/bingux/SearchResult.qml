import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets

Item {
    id: root

    required property var result
    required property string query
    required property bool selected
    property bool activationEnabled: true
    property bool previewAvailable: false
    property bool previewOpen: false
    property var claimChevronAnimation: null
    property bool componentReady: false
    Component.onCompleted: {
        componentReady = true;
        updateChevron();
    }
    onSelectedChanged: if (componentReady)
        updateChevron()
    function updateChevron() {
        chevronEntrance.stop();
        if (!selected) {
            selectionChevron.opacity = 0;
            chevronOffset.x = -24;
            return;
        }
        const animate = claimChevronAnimation ? claimChevronAnimation() : true;
        if (animate && !Theme.reducedMotion && !webResult)
            chevronEntrance.restart();
        else {
            selectionChevron.opacity = 1;
            chevronOffset.x = 0;
        }
    }

    signal activated(var position)
    signal contextMenuRequested(var position)
    signal previewToggled
    signal hovered

    readonly property bool featured: result.kind === "weather" || result.kind === "calculation" || result.kind === "chat"
    readonly property bool webResult: result.providerId === "web" || result.providerId === "web-suggestions"
    readonly property bool fileResult: result.providerId === "files" && (result.kind === "file" || result.kind === "folder") && String(result.subtitle || "").startsWith("/")
    readonly property bool symbolic: !fileResult && !webResult && (featured || String(result.icon || "").endsWith("-symbolic"))
    readonly property Item activationIcon: featured ? featuredIconSurface : standardIconSurface
    readonly property string activationIconSource: fileResult ? "file-preview:" + encodeURIComponent(result.subtitle) : symbolic ? symbolicSource : webResult ? (Quickshell.iconPath("duckduckgo", true) || Qt.resolvedUrl("icons/duckduckgo.svg")) : Quickshell.iconPath(result.icon || defaultIcon(), defaultIcon())
    readonly property int rowHeight: webResult ? 38 : result.kind === "calculation" ? 78 : result.kind === "weather" ? 84 : result.kind === "chat" ? 68 : Theme.searchResultHeight
    readonly property string symbolicSource: {
        if (!symbolic)
            return "";
        const icon = result.icon || defaultIcon();
        const themed = Quickshell.iconPath(icon.endsWith("-symbolic") ? icon : icon + "-symbolic", true);
        if (themed)
            return themed;
        if (result.kind === "file")
            return Quickshell.iconPath(icon === "application-pdf" ? "x-office-document-symbolic" : "text-x-generic-symbolic");
        return Quickshell.iconPath(defaultIcon().replace(/-symbolic$/, "") + "-symbolic", "system-search-symbolic");
    }
    readonly property string providerLabel: {
        const labels = {
            "applications": "Applications",
            "files": "Files",
            "calculation": "Calculator",
            "weather": "Weather",
            "ai": "Assistant"
        };
        if (labels[result.providerId])
            return labels[result.providerId];

        const words = String(result.providerId || "Search").split("-");
        let label = "";
        for (let index = 0; index < words.length; index += 1) {
            if (words[index] === "")
                continue;
            label += (label === "" ? "" : " ") + words[index].charAt(0).toUpperCase() + words[index].slice(1);
        }
        return label || "Search";
    }

    width: parent ? parent.width : 0
    height: rowHeight
    implicitHeight: rowHeight
    opacity: activationEnabled ? 1 : 0.58

    function escapeRichText(value) {
        return String(value || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/\"/g, "&quot;");
    }

    // Mirror the daemon's lexical rules for presentation only; operators and
    // exclusions must never become highlighted text or protected path spans.
    function queryTerms() {
        const input = String(root.query || "").trim().replace(/^\?\s*/, "");
        const terms = [];
        let i = 0;
        while (i < input.length) {
            if (/\s/.test(input[i])) {
                i++;
                continue;
            }
            const excluded = input[i] === "-" && i + 1 < input.length && !/\s/.test(input[i + 1]);
            if (excluded)
                i++;
            let quoted = input[i] === '"';
            let exact = quoted;
            let value = quoted ? "" : input[i];
            i++;
            while (i < input.length && (quoted || !/\s/.test(input[i]))) {
                const next = input[i++];
                if (next === "\\" && (input[i] === '"' || input[i] === "\\"))
                    value += input[i++];
                else if (next === '"') {
                    quoted = !quoted;
                    exact = true;
                } else
                    value += next;
            }
            if (!value || excluded || (!exact && (value === "OR" || value === "AND" || /^(filetype|ext):/i.test(value))))
                continue;
            terms.push({
                value: value.toLowerCase(),
                exact: exact
            });
        }
        return terms;
    }

    function matchMask(value) {
        const text = String(value || "");
        const marked = new Array(text.length).fill(false);
        const trimmedQuery = String(root.query || "").trim().replace(/^\?\s*/, "");
        if (trimmedQuery === "")
            return marked;

        const lowerText = text.toLowerCase();
        for (const token of queryTerms()) {
            const term = token.value;
            if (!term)
                continue;
            let start = lowerText.indexOf(term);
            if (start >= 0) {
                while (start >= 0) {
                    for (let i = start; i < start + term.length; i++)
                        marked[i] = true;
                    start = lowerText.indexOf(term, start + term.length);
                }
            } else if (!token.exact) {
                // Match the ordered characters used by fuzzy application search.
                const positions = [];
                let cursor = 0;
                for (const character of term) {
                    const position = lowerText.indexOf(character, cursor);
                    if (position < 0)
                        break;
                    positions.push(position);
                    cursor = position + character.length;
                }
                if (positions.length === Array.from(term).length && positions[positions.length - 1] - positions[0] + 1 - term.length <= Array.from(term).length * 2)
                    for (const position of positions)
                        marked[position] = true;
            }
        }
        return marked;
    }

    function styledText(text, marked) {
        let output = "";
        for (let i = 0; i < text.length; i++) {
            if (marked[i] && !marked[i - 1])
                output += '<font color="' + Theme.searchAccent + '"><b>';
            output += escapeRichText(text[i]);
            if (marked[i] && !marked[i + 1])
                output += "</b></font>";
        }
        return output;
    }

    function highlightedText(value) {
        const text = String(value || "");
        return styledText(text, matchMask(text));
    }

    // Remove low-value path context first, without ever removing a match.
    // Measure whole rendered runs so proportional fonts and bold matches count.
    function compactPath(value, availableWidth, measure) {
        const text = String(value || "");
        const marked = matchMask(text);
        const kept = new Array(text.length).fill(true);
        const basename = text.lastIndexOf("/") + 1;
        function render() {
            let plain = "";
            const marks = [];
            let omitted = false;
            for (let i = 0; i < text.length; i++) {
                if (!kept[i]) {
                    if (!omitted) {
                        plain += "…";
                        marks.push(false);
                    }
                    omitted = true;
                } else {
                    plain += text[i];
                    marks.push(marked[i]);
                    omitted = false;
                }
            }
            return {
                plain: plain,
                rich: styledText(plain, marks)
            };
        }
        let rendered = render();
        if (measure(rendered.rich) <= availableWidth)
            return rendered.rich;
        const removable = [];
        for (let i = 0; i < text.length; i++) {
            if (marked[i])
                continue;
            // Keep the filename and the root separator longest. Within the
            // filename, shorten its middle before its beginning or extension.
            const priority = i === 0 ? 3 : i >= basename ? 2 : 1;
            const distance = i >= basename ? Math.min(i - basename, text.length - 1 - i) : Math.min(i, basename - 1 - i);
            removable.push({
                index: i,
                priority: priority,
                distance: distance
            });
        }
        removable.sort((a, b) => a.priority - b.priority || b.distance - a.distance || a.index - b.index);
        for (const entry of removable) {
            // Never split a UTF-16 surrogate pair.
            let i = entry.index;
            kept[i] = false;
            const code = text.charCodeAt(i);
            if (code >= 0xd800 && code <= 0xdbff && !marked[i + 1])
                kept[i + 1] = false;
            if (code >= 0xdc00 && code <= 0xdfff && !marked[i - 1])
                kept[i - 1] = false;
            rendered = render();
            if (measure(rendered.rich) <= availableWidth)
                break;
        }
        return rendered.rich;
    }

    function defaultIcon() {
        if (result.kind === "application")
            return "application-x-executable";
        if (result.kind === "folder")
            return "folder";
        if (result.kind === "file")
            return "text-x-generic";
        if (result.kind === "calculation")
            return "accessories-calculator";
        if (result.kind === "weather")
            return "weather-clear";
        if (result.kind === "chat")
            return "dialog-question-symbolic";
        if (result.kind === "action")
            return "system-run-symbolic";
        return "system-search-symbolic";
    }

    Rectangle {
        id: rowSurface

        anchors.fill: parent
        radius: Theme.insetRadius(Theme.radius + 2, 2)
        color: root.selected ? Theme.searchSelection : "transparent"
        border.width: root.selected ? 1 : 0
        border.color: "#387e9fc8"
    }

    Item {
        id: standardContent

        visible: !root.featured
        anchors.fill: parent

        Item {
            id: standardIconSurface

            width: root.webResult ? 20 : 34
            height: width

            anchors {
                left: parent.left
                leftMargin: root.webResult ? 21 : 14
                verticalCenter: parent.verticalCenter
            }

            OsIconImage {
                anchors.fill: parent
                visible: !root.symbolic
                source: root.symbolic ? "" : root.activationIconSource
            }
            SymbolicIcon {
                anchors.fill: parent
                visible: root.symbolic
                source: root.symbolicSource
                color: Theme.muted
            }
        }

        Column {
            spacing: 2

            anchors {
                left: standardIconSurface.right
                right: parent.right
                leftMargin: root.webResult ? 19 : 12
                rightMargin: root.webResult ? engineLabel.width + 26 : root.previewAvailable && root.selected ? 96 : 34
                verticalCenter: parent.verticalCenter
            }

            MarqueeText {
                objectName: "searchTitleMarquee"
                width: parent.width
                height: implicitHeight
                color: Theme.text
                active: root.selected && root.activationEnabled
                pixelSize: Theme.fontSize
                fontWeight: Font.Medium
                text: root.highlightedText(result.title)
            }

            MarqueeText {
                id: subtitleViewport
                objectName: "searchPathMarquee"
                width: parent.width
                visible: !root.webResult && result.subtitle !== ""
                height: implicitHeight
                active: root.selected && root.activationEnabled && visible
                color: Theme.muted
                pixelSize: Theme.fontSmall
                text: root.highlightedText(root.result.subtitle)
                property var pathInputs: [root.result.subtitle, root.result.kind, root.query, width, Theme.fontFamily, Theme.fontSmall]
                onPathInputsChanged: pathUpdate.restart()
                Component.onCompleted: updatePath()
                // Owned by the delegate so pending work disappears with it
                // when a new query replaces the rows or the popout closes.
                Timer {
                    id: pathUpdate
                    interval: 0
                    onTriggered: subtitleViewport.updatePath()
                }
                function updatePath() {
                    restText = root.result.kind === "file" || root.result.kind === "folder" ? root.compactPath(root.result.subtitle, width, function (rich) {
                        pathMeasure.text = rich;
                        return pathMeasure.implicitWidth;
                    }) : root.highlightedText(root.result.subtitle);
                }
            }
        }

        Text {
            id: engineLabel
            visible: root.webResult
            anchors.right: parent.right
            anchors.rightMargin: 14
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(100, implicitWidth)
            text: root.webResult ? "DuckDuckGo" : ""
            textFormat: Text.PlainText
            font.family: Theme.fontFamily
            font.pixelSize: 11
            color: Theme.muted
            elide: Text.ElideRight
        }

        Text {
            id: previewLabel
            x: parent.width - 14 - (root.previewOpen ? 0 : 24) - width
            width: implicitWidth
            horizontalAlignment: Text.AlignRight
            anchors.verticalCenter: parent.verticalCenter
            visible: root.previewAvailable && root.selected
            objectName: "searchPreviewLabel"
            Behavior on x {
                NumberAnimation {
                    duration: Theme.previewMotion
                    easing.type: Easing.OutCubic
                }
            }
            onTextChanged: labelFade.restart()
            NumberAnimation {
                id: labelFade
                target: previewLabel
                property: "opacity"
                from: 0
                to: 1
                duration: Theme.previewMotion
            }
            text: root.previewOpen ? "Hide" : "Preview"
            color: Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: 11
        }

        SymbolicIcon {
            id: selectionChevron
            objectName: "searchSelectionChevron"
            x: root.previewOpen && root.previewAvailable ? parent.width - 14 - previewLabel.width - 8 - width : parent.width - 30
            anchors.verticalCenter: parent.verticalCenter
            rotation: root.previewOpen && root.previewAvailable ? 180 : 0
            Behavior on x {
                NumberAnimation {
                    duration: Theme.previewMotion
                    easing.type: Easing.OutCubic
                }
            }
            Behavior on rotation {
                NumberAnimation {
                    duration: Theme.previewMotion
                    easing.type: Easing.OutCubic
                }
            }
            visible: opacity > 0 && !root.webResult
            opacity: 0
            transform: Translate {
                id: chevronOffset
                x: -24
            }
            implicitSize: 16
            source: Quickshell.iconPath("go-next-symbolic")
            color: Theme.muted
        }

        ParallelAnimation {
            id: chevronEntrance
            NumberAnimation {
                target: selectionChevron
                property: "opacity"
                from: 0
                to: 1
                duration: 230
                easing.type: Easing.OutCubic
            }
            NumberAnimation {
                target: chevronOffset
                property: "x"
                from: -24
                to: 0
                duration: 230
                easing.type: Easing.OutCubic
            }
        }
    }

    Text {
        id: pathMeasure
        visible: false
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSmall
        textFormat: Text.StyledText
    }

    Item {
        id: featuredContent

        visible: root.featured
        anchors.fill: parent

        Item {
            id: featuredIconSurface

            width: root.result.kind === "calculation" ? 50 : 46
            height: width

            anchors {
                left: parent.left
                leftMargin: 14
                verticalCenter: parent.verticalCenter
            }

            SymbolicIcon {
                anchors.fill: parent
                source: root.symbolicSource
                color: Theme.muted
            }
        }

        Column {
            spacing: root.result.kind === "calculation" ? 0 : 2

            anchors {
                left: featuredIconSurface.right
                right: parent.right
                leftMargin: 14
                rightMargin: 16
                verticalCenter: parent.verticalCenter
            }

            Text {
                width: parent.width
                color: root.result.kind === "calculation" ? Theme.searchAccent : Theme.muted
                elide: Text.ElideRight
                font.family: Theme.fontFamily
                font.pixelSize: root.result.kind === "calculation" ? 23 : 11
                font.weight: Font.DemiBold
                text: root.highlightedText(result.title)
                textFormat: Text.StyledText
            }

            Text {
                width: parent.width
                visible: result.subtitle !== ""
                color: root.result.kind === "calculation" ? Theme.muted : Theme.text
                elide: root.result.kind === "weather" ? Text.ElideNone : Text.ElideRight
                font.family: Theme.fontFamily
                font.pixelSize: root.result.kind === "calculation" ? Theme.fontSmall : Theme.fontSize
                font.weight: root.result.kind === "chat" ? Font.DemiBold : Font.Normal
                maximumLineCount: root.result.kind === "weather" ? 2 : 1
                text: root.highlightedText(result.subtitle)
                textFormat: Text.StyledText
                wrapMode: root.result.kind === "weather" ? Text.WordWrap : Text.NoWrap
            }

            Text {
                visible: root.result.kind !== "calculation"
                color: root.selected ? Theme.searchAccent : Theme.muted
                elide: Text.ElideRight
                font.family: Theme.fontFamily
                font.pixelSize: 11
                text: root.providerLabel
                textFormat: Text.PlainText
            }
        }
    }

    MouseArea {
        id: rowMouse

        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.ArrowCursor
        // Keep pointer tracking alive while queries refresh; only activation
        // is gated. Re-enabling a MouseArea under a still pointer loses entry.
        enabled: true
        hoverEnabled: true
        onWheel: wheel => {
            const point = subtitleViewport.mapFromItem(rowMouse, wheel.x, wheel.y);
            const overflow = subtitleViewport.overflow;
            if (subtitleViewport.visible && overflow > 0 && point.y >= 0 && point.y <= subtitleViewport.height) {
                const delta = wheel.pixelDelta.x || wheel.pixelDelta.y || wheel.angleDelta.x / 3 || wheel.angleDelta.y / 3;
                subtitleViewport.scrollBy(-delta);
                wheel.accepted = true;
            } else {
                wheel.accepted = false;
            }
        }
        onClicked: mouse => {
            if (!root.activationEnabled)
                return;
            if (mouse.button === Qt.RightButton) {
                root.contextMenuRequested(Qt.point(mouse.x, mouse.y));
                return;
            }
            if (root.previewAvailable && root.selected && mouse.x >= width - 96)
                root.previewToggled();
            else
                root.activated(Qt.point(mouse.x, mouse.y));
        }
        // Moving the mouse selects a row; a stationary pointer must not undo
        // keyboard selection when result delegates are recreated.
        onPositionChanged: if (containsMouse)
            root.hovered()
        onEntered: root.hovered()
    }
}
