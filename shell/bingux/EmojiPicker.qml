import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import "EmojiSearch.js" as EmojiSearch
import "PopupPlacement.js" as PopupPlacement

ShellPopup {
    id: root
    popupWidth: 440
    popupHeight: 408
    preferredX: popupPosition.x
    preferredY: popupPosition.y
    property var popupPosition: ({
            x: (width - popupWidth) / 2,
            y: (height - popupHeight) * 0.32
        })
    property bool locating: false
    property var inputAnchor: null
    property var caretResult: null
    property string anchorSource: "fallback"
    revealOriginY: popupHeight / 2
    property bool shortcutEnabled: true
    property bool copyOnSelect: false
    property bool insertOnSelect: true
    property string lastFocusedWindow: ""
    property string lastFocusedTitle: ""
    property string targetWindow: ""
    property string pendingText: ""
    property bool inserting: false
    property string stateDirectory: Quickshell.statePath("emoji")
    property bool persistRecent: true
    property var catalogue: []
    property var recent: []
    property bool recentReady: false
    property int skinTone: 0
    property bool skinToneReady: false
    readonly property var skinTones: ["✋", "✋🏻", "✋🏼", "✋🏽", "✋🏾", "✋🏿"]
    readonly property var skinToneNames: ["Default", "Light", "Medium-light", "Medium", "Medium-dark", "Dark"]
    onSkinToneChanged: if (persistRecent && skinToneReady)
        tonePreference.setText(JSON.stringify(skinTone))
    property string query: ""
    property string category: ""
    property string error: ""
    property int selectedIndex: 0
    property int activationCount: 0
    readonly property double instanceToken: Date.now()
    readonly property bool shortcutReady: shortcut.ready
    readonly property var results: EmojiSearch.search(catalogue, query, category, recent, skinTone)
    readonly property var selectedEmoji: results[selectedIndex] || null
    readonly property var categories: [
        {
            id: "",
            label: "All emoji",
            glyph: "⌘"
        },
        {
            id: "recent",
            label: "Recently used",
            glyph: "◷"
        },
        {
            id: "Smileys & Emotion",
            label: "Smileys",
            glyph: "😀"
        },
        {
            id: "People & Body",
            label: "People",
            glyph: "👋"
        },
        {
            id: "Animals & Nature",
            label: "Nature",
            glyph: "🌿"
        },
        {
            id: "Food & Drink",
            label: "Food",
            glyph: "🍋"
        },
        {
            id: "Travel & Places",
            label: "Travel",
            glyph: "🚀"
        },
        {
            id: "Activities",
            label: "Activities",
            glyph: "⚽"
        },
        {
            id: "Objects",
            label: "Objects",
            glyph: "💡"
        },
        {
            id: "Symbols",
            label: "Symbols",
            glyph: "💛"
        },
        {
            id: "Flags",
            label: "Flags",
            glyph: "🏁"
        }
    ]
    signal opening
    signal chosen(string emoji)
    function open() {
        if (visible) {
            visible = false;
            return;
        }
        if (pendingText || locating)
            return;
        targetWindow = lastFocusedWindow;
        if (!hostItem && shortcut.connected) {
            locating = true;
            inputAnchor = null;
            caretResult = null;
            anchorSource = "fallback";
            anchorDeadline.restart();
            shortcut.requestInputAnchor();
        } else
            showPicker();
    }
    function finishPlacement() {
        if (!locating && !visible)
            return;
        const firstPlacement = locating;
        locating = false;
        anchorDeadline.stop();
        if (inputAnchor) {
            let anchor = {
                x: inputAnchor.x,
                y: inputAnchor.y,
                height: 1
            };
            const frame = inputAnchor.frame;
            const caret = inputAnchor.caret || PopupPlacement.desktopCaret(caretResult, inputAnchor);
            // Reject stale, app-relative or otherwise unusable accessibility geometry.
            if (caret && frame && Number.isFinite(caret.x) && Number.isFinite(caret.y) && caret.height > 0 && caret.height < 200 && caret.x >= frame.x && caret.x <= frame.x + frame.width && caret.y >= frame.y && caret.y + caret.height <= frame.y + frame.height)
                anchor = caret;
            const output = Quickshell.screens.find(s => anchor.x >= s.x && anchor.x < s.x + s.width && anchor.y >= s.y && anchor.y < s.y + s.height);
            if (output) {
                anchorSource = anchor.source || "pointer";
                root.screen = output;
                popupPosition = PopupPlacement.place(anchor, output, popupWidth, popupHeight, Theme.gap, 12);
            }
        }
        if (firstPlacement)
            showPicker();
    }
    function close() {
        locating = false;
        anchorDeadline.stop();
        visible = false;
    }
    function showPicker() {
        query = "";
        category = "";
        selectedIndex = 0;
        error = "";
        opening();
        visible = true;
        focusInput.restart();
    }
    Timer {
        id: anchorDeadline
        interval: 220
        onTriggered: root.finishPlacement()
    }
    Process {
        id: caretProbe
        command: (Quickshell.env("BINGUX_CARET_HELPER") ? [Quickshell.env("BINGUX_CARET_HELPER")] : ["python3", decodeURIComponent(Qt.resolvedUrl("caret-anchor.py").toString().replace(/^file:\/\//, ""))]).concat([String(root.inputAnchor ? root.inputAnchor.pid : 0)])
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.caretResult = JSON.parse(text);
                } catch (_) {
                    root.caretResult = null;
                }
            }
        }
        onExited: root.finishPlacement()
    }
    function choose() {
        if (!selectedEmoji)
            return;
        const emoji = selectedEmoji.emoji;
        if (insertOnSelect && (!targetWindow || !shortcut.connected)) {
            error = "Focus an app's text field, then open the picker.";
            return;
        }
        if (copyOnSelect)
            Quickshell.clipboardText = emoji;
        recent = EmojiSearch.remember(recent, emoji);
        if (persistRecent && recentReady)
            history.setText(JSON.stringify(recent));
        chosen(emoji);
        if (insertOnSelect) {
            pendingText = emoji;
            beginInsertion();
        }
    }
    onRetainedChanged: if (!retained && pendingText && !inserting)
        beginInsertion()
    function beginInsertion() {
        if (!pendingText || !targetWindow || !shortcut.connected)
            return;
        inserting = true;
        // Release the popup's keyboard grab while the compositor commits the
        // text to the original input. The picker stays mapped and visible.
        keyboardInteractive = false;
        shortcut.activateWindow(targetWindow);
        insertDelay.restart();
    }
    Timer {
        id: insertDelay
        interval: 80
        onTriggered: if (root.pendingText) {
            shortcut.insertText(root.targetWindow, root.pendingText);
            insertTimeout.restart();
        }
    }
    Timer {
        id: insertTimeout
        interval: 5000
        onTriggered: root.insertionFailed("The app did not accept the insertion request. Please try again.")
    }
    function insertionFailed(message) {
        insertDelay.stop();
        insertTimeout.stop();
        pendingText = "";
        inserting = false;
        keyboardInteractive = true;
        error = message;
        visible = true;
        focusInput.restart();
    }
    function insertionFinished() {
        pendingText = "";
        insertTimeout.stop();
        inserting = false;
        keyboardInteractive = true;
        focusInput.restart();
    }
    function moveSelection(delta) {
        selectedIndex = Math.max(0, Math.min(results.length - 1, selectedIndex + delta));
        const top = Math.floor(selectedIndex / 8) * grid.cellHeight;
        const bottom = top + grid.cellHeight;
        let destination = grid.contentY;
        if (top < destination)
            destination = top;
        else if (bottom > destination + grid.height)
            destination = bottom - grid.height;
        destination = Math.max(0, Math.min(Math.max(0, grid.contentHeight - grid.height), destination));
        gridScroll.stop();
        if (Theme.reducedMotion)
            grid.contentY = destination;
        else {
            gridScroll.to = destination;
            gridScroll.restart();
        }
    }
    NumberAnimation {
        id: gridScroll
        target: grid
        property: "contentY"
        duration: 140
        easing.type: Easing.OutCubic
    }
    function focusCategory() {
        const index = Math.max(0, categories.findIndex(item => item.id === category));
        categoryButtons.itemAt(index).forceActiveFocus(Qt.TabFocusReason);
    }
    function moveCategory(delta) {
        const current = Math.max(0, categories.findIndex(item => item.id === category));
        category = categories[(current + delta + categories.length) % categories.length].id;
        focusCategory();
    }
    function enterGrid() {
        selectedIndex = 0;
        grid.positionViewAtBeginning();
        input.forceActiveFocus(Qt.TabFocusReason);
    }
    onVisibleChanged: if (!visible)
        tonePopup.close()
    onResultsChanged: {
        gridScroll.stop();
        selectedIndex = 0;
        grid.positionViewAtBeginning();
    }
    Timer {
        id: focusInput
        interval: 0
        onTriggered: if (root.visible)
            input.forceActiveFocus()
    }
    ShortcutSession {
        id: shortcut
        onInputAnchor: anchor => {
            if (!root.locating)
                return;
            root.inputAnchor = anchor;
            if (anchor.window)
                root.targetWindow = anchor.window;
            // Accept input at the compositor anchor immediately. AT-SPI can
            // refine placement later without delaying or resetting the query.
            if (!(anchor.caret && anchor.caret.height > 0) && anchor.pid > 0 && !caretProbe.running)
                caretProbe.running = true;
            root.finishPlacement();
        }
        enabled: root.shortcutEnabled
        trackWindows: root.insertOnSelect
        onWindowSnapshot: windows => {
            if (!root.visible && !root.pendingText) {
                const focused = windows.find(window => window.focused);
                root.lastFocusedWindow = focused ? focused.id : "";
                root.lastFocusedTitle = focused ? focused.title : "";
            }
        }
        onTextInserted: if (root.inserting)
            root.insertionFinished()
        // Popup commands are registered in init.lua through binguxctl.
        // Keep this transport for window tracking, caret lookup and insertion.
        onFailed: message => {
            if (root.pendingText)
                root.insertionFailed(message);
            else
                console.warn("Emoji shortcut:", message);
        }
    }
    IpcHandler {
        target: "emoji"
        function open(): void {
            if (!root.visible && !root.locating) {
                root.activationCount++;
                root.open();
            }
        }
        function close(): void {
            root.close();
        }
        function status(): string {
            return JSON.stringify({
                visible: root.visible,
                ready: root.shortcutReady,
                count: root.catalogue.length,
                activations: root.activationCount,
                instance: root.instanceToken,
                query: root.query,
                selected: root.selectedEmoji ? root.selectedEmoji.emoji : "",
                error: root.error,
                anchor: root.anchorSource,
                x: root.panelX,
                y: root.panelY,
                screenX: root.screen ? root.screen.x : 0,
                screenY: root.screen ? root.screen.y : 0
            });
        }
    }
    FileView {
        path: decodeURIComponent(Qt.resolvedUrl("emoji-data.json").toString().replace(/^file:\/\//, ""))
        onLoaded: {
            try {
                root.catalogue = EmojiSearch.foldSkinTones(JSON.parse(text()).emoji);
            } catch (_) {
                root.error = "Could not load emoji.";
            }
        }
        onLoadFailed: root.error = "Could not load emoji."
    }
    Process {
        command: ["mkdir", "-p", "-m", "700", root.stateDirectory]
        running: root.persistRecent
        onExited: code => {
            if (code === 0) {
                history.path = root.stateDirectory + "/recent.json";
                tonePreference.path = root.stateDirectory + "/skin-tone.json";
            }
        }
    }
    FileView {
        id: history
        atomicWrites: true
        printErrors: false
        onLoaded: {
            try {
                const data = JSON.parse(text());
                if (Array.isArray(data))
                    root.recent = data.filter(value => typeof value === "string" && value.length < 64).slice(0, 32);
            } catch (_) {}
            root.recentReady = true;
        }
        onLoadFailed: root.recentReady = true
    }
    FileView {
        id: tonePreference
        atomicWrites: true
        printErrors: false
        onLoaded: {
            try {
                const tone = JSON.parse(text());
                if (Number.isInteger(tone) && tone >= 0 && tone <= 5)
                    root.skinTone = tone;
            } catch (_) {}
            root.skinToneReady = true;
        }
        onLoadFailed: root.skinToneReady = true
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: Theme.gap
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.gap
            TextField {
                id: input
                objectName: "emojiSearch"
                selectionColor: Theme.textSelection
                selectedTextColor: Theme.text
                Layout.fillWidth: true
                implicitHeight: 40
                padding: 12
                text: root.query
                onTextEdited: root.query = text
                placeholderText: "Search emoji…"
                color: Theme.text
                placeholderTextColor: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize
                background: Rectangle {
                    radius: 8
                    color: Theme.elevated
                    border.width: input.activeFocus ? 1 : 0
                    border.color: Theme.outline
                }
                Keys.onDownPressed: root.moveSelection(8)
                Keys.onUpPressed: {
                    if (root.selectedIndex < 8)
                        root.focusCategory();
                    else
                        root.moveSelection(-8);
                }
                Keys.onTabPressed: toneButton.forceActiveFocus(Qt.TabFocusReason)
                Keys.onLeftPressed: root.moveSelection(-1)
                Keys.onRightPressed: root.moveSelection(1)
                Keys.onReturnPressed: root.choose()
                Keys.onEnterPressed: root.choose()
                Keys.onEscapePressed: root.visible = false
            }
            AbstractButton {
                id: toneButton
                objectName: "emojiSkinTone"
                implicitWidth: 40
                implicitHeight: 40
                hoverEnabled: true
                Accessible.name: "Skin tone: " + root.skinToneNames[root.skinTone]
                onClicked: tonePopup.open()
                Keys.onReturnPressed: tonePopup.open()
                Keys.onEnterPressed: tonePopup.open()
                Keys.onTabPressed: root.focusCategory()
                Keys.onBacktabPressed: input.forceActiveFocus(Qt.BacktabFocusReason)
                Keys.onEscapePressed: root.visible = false
                background: ControlCentreButtonSurface {
                    control: toneButton
                    selected: tonePopup.opened
                    radius: 8
                }
                contentItem: Text {
                    text: root.skinTones[root.skinTone]
                    font.family: "Noto Color Emoji"
                    font.pixelSize: 24
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                ShellTooltip {
                    parent: toneButton
                    visible: toneButton.hovered && !tonePopup.opened
                    text: "Skin tone"
                }
                Popup {
                    id: tonePopup
                    popupType: Popup.Item
                    x: toneButton.width - width
                    y: toneButton.height + 4
                    padding: 6
                    focus: true
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                    onOpened: toneOptions.itemAt(root.skinTone).forceActiveFocus()
                    onClosed: if (root.visible)
                        input.forceActiveFocus()
                    background: Rectangle {
                        color: Theme.elevated
                        radius: 10
                        border.width: 1
                        border.color: Theme.outline
                    }
                    contentItem: Row {
                        spacing: 2
                        Repeater {
                            id: toneOptions
                            model: root.skinTones
                            AbstractButton {
                                id: toneOption
                                required property string modelData
                                required property int index
                                objectName: "emojiTone" + index
                                width: 36
                                height: 38
                                hoverEnabled: true
                                Accessible.name: root.skinToneNames[index] + " skin tone"
                                onClicked: {
                                    root.skinTone = index;
                                    tonePopup.close();
                                }
                                Keys.onLeftPressed: toneOptions.itemAt((index + 5) % 6).forceActiveFocus()
                                Keys.onRightPressed: toneOptions.itemAt((index + 1) % 6).forceActiveFocus()
                                Keys.onReturnPressed: clicked()
                                Keys.onEnterPressed: clicked()
                                background: ControlCentreButtonSurface {
                                    control: toneOption
                                    selected: root.skinTone === toneOption.index
                                    radius: 6
                                }
                                contentItem: Text {
                                    text: toneOption.modelData
                                    font.family: "Noto Color Emoji"
                                    font.pixelSize: 24
                                    horizontalAlignment: Text.AlignHCenter
                                    verticalAlignment: Text.AlignVCenter
                                }
                            }
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 2
            Repeater {
                id: categoryButtons
                model: root.categories
                AbstractButton {
                    id: categoryButton
                    required property var modelData
                    required property int index
                    objectName: "emojiCategory" + index
                    focusPolicy: root.category === modelData.id ? Qt.StrongFocus : Qt.ClickFocus
                    Layout.fillWidth: true
                    Layout.preferredWidth: 0
                    implicitHeight: 32
                    hoverEnabled: true
                    Accessible.name: modelData.label
                    onClicked: {
                        root.category = modelData.id;
                        input.forceActiveFocus();
                    }
                    Keys.onLeftPressed: root.moveCategory(-1)
                    Keys.onRightPressed: root.moveCategory(1)
                    Keys.onDownPressed: root.enterGrid()
                    Keys.onUpPressed: input.forceActiveFocus(Qt.TabFocusReason)
                    Keys.onTabPressed: root.enterGrid()
                    Keys.onBacktabPressed: input.forceActiveFocus(Qt.TabFocusReason)
                    Keys.onReturnPressed: root.enterGrid()
                    Keys.onEnterPressed: root.enterGrid()
                    Keys.onEscapePressed: root.visible = false
                    background: ControlCentreButtonSurface {
                        control: categoryButton
                        selected: root.category === categoryButton.modelData.id
                    }
                    contentItem: Item {
                        SymbolicIcon {
                            anchors.centerIn: parent
                            visible: categoryButton.modelData.id === "" || categoryButton.modelData.id === "recent"
                            implicitSize: 16
                            source: Quickshell.iconPath(categoryButton.modelData.id === "recent" ? "document-open-recent-symbolic" : "view-grid-symbolic")
                            color: Theme.text
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: categoryButton.modelData.id !== "" && categoryButton.modelData.id !== "recent"
                            text: categoryButton.modelData.glyph
                            font.pixelSize: 18
                        }
                    }
                    ShellTooltip {
                        parent: categoryButton
                        visible: categoryButton.hovered
                        text: categoryButton.modelData.label
                    }
                }
            }
        }
        GridView {
            id: grid
            objectName: "emojiGrid"
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            cellWidth: width / 8
            cellHeight: 48
            model: root.results
            boundsBehavior: Flickable.StopAtBounds
            onMovementStarted: gridScroll.stop()
            ScrollBar.vertical: ScrollBar {}
            delegate: AbstractButton {
                id: emojiButton
                required property var modelData
                required property int index
                width: grid.cellWidth - 2
                height: grid.cellHeight - 2
                hoverEnabled: true
                focusPolicy: Qt.NoFocus
                Accessible.name: modelData.name
                onHoveredChanged: if (hovered)
                    root.selectedIndex = index
                onClicked: {
                    root.selectedIndex = index;
                    root.choose();
                }
                background: ControlCentreButtonSurface {
                    control: emojiButton
                    selected: emojiButton.index === root.selectedIndex
                    radius: 8
                }
                contentItem: Text {
                    text: emojiButton.modelData.emoji
                    font.family: "Noto Color Emoji"
                    font.pixelSize: 28
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
            }
            Text {
                anchors.centerIn: parent
                visible: grid.count === 0
                text: root.error || (root.category === "recent" && !root.query ? "Your recent emoji will appear here" : "No matching emoji")
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSize
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                text: root.error || (root.selectedEmoji ? root.selectedEmoji.name : "")
                elide: Text.ElideRight
                color: root.error ? "#ff737d" : Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
            Text {
                text: root.insertOnSelect ? "Enter to insert" : "Enter to copy"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: Theme.fontSmall
            }
        }
    }
}
