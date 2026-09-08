import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtCore
import Quickshell
import Bingux.Text 1.0

Item {
    id: root
    property var screen: Quickshell.screens[0]
    property Item menuHost: null
    function closeContextMenu() { if (contextMenuLoader.item) contextMenuLoader.item.visible = false; }
    onVisibleChanged: if (!visible) { closeContextMenu(); slashMenu.close(); }
    onMenuHostChanged: slashMenu.close()
    function showContextMenu(position) {
        slashMenu.close();
        if (editor.selectionStart === editor.selectionEnd)
            editor.cursorPosition = editor.positionAt(position.x, position.y);
        const point = menuHost ? editor.mapToItem(menuHost, position.x, position.y) : editor.mapToGlobal(position.x, position.y);
        if (!contextMenuLoader.item) contextMenuLoader.setSource("NotesContextMenu.qml", {notes: root, editor: editor});
        const contextMenu = contextMenuLoader.item;
        if (!contextMenu) return;
        contextMenu.preferredX = point.x;
        contextMenu.preferredY = point.y;
        contextMenu.visible = true;
        Qt.callLater(contextMenu.focusMenu);
    }
    property var formatUndoGroups: []
    function undoEdit(redo) {
        const current = editor.text;
        const groups = formatUndoGroups.slice().reverse();
        const group = groups.find(entry => (redo ? entry.before : entry.after) === current);
        const operation = redo ? "redo" : "undo";
        if (!(redo ? editor.canRedo : editor.canUndo)) return;
        const limit = group ? group.steps : 1;
        for (let step = 0; step < limit; step++) {
            editor[operation]();
            if (!group || editor.text === (redo ? group.after : group.before)) break;
            if (!group.states.includes(editor.text) || !(redo ? editor.canRedo : editor.canUndo)) break;
        }
        save();
    }
    function menuEnabled(action) {
        if (action === "undo") return editor.canUndo;
        if (action === "redo") return editor.canRedo;
        if (["cut", "copy"].includes(action)) return editor.selectionStart !== editor.selectionEnd;
        if (action === "paste") return editor.canPaste;
        if (action === "selectAll") return editor.length > 0;
        return true;
    }
    function applyAction(action) {
        closeContextMenu();
        editor.continuation = null;
        if (action === "undo" || action === "redo") undoEdit(action === "redo");
        else if (["cut", "copy", "paste", "selectAll"].includes(action)) editor[action]();
        else if (["bold", "italic", "strikeout"].includes(action)) editor.cursorSelection.font[action] = !editor.cursorSelection.font[action];
        else editor.formatBlock(action);
        save();
        Qt.callLater(root.focusContent);
    }
    function insertCommand(command, start, end) {
        editor.continuation = null;
        let markdown = command.markdown;
        let block = !!command.block;
        let selection = command.select || "";
        if (command.input) {
            // Leave the URL editable until Enter applies the Markdown fragment.
            markdown = command.input.replace(/([\\`*_{}\[\]()#+.!>|~-])/g, "\\$1");
        } else if (command.id === "date") {
            markdown = Qt.formatDate(new Date(), "yyyy-MM-dd");
        } else if (command.id === "duplicate" || command.id === "delete") {
            const before = editor.getText(0, start);
            const blockStart = Math.max(before.lastIndexOf("\n"), before.lastIndexOf("\u2029")) + 1;
            const after = editor.getText(end, editor.length);
            const boundary = after.search(/[\n\u2029]/);
            const blockEnd = boundary < 0 ? editor.length : end + boundary;
            markdown = (editor.getFormattedText(blockStart, start) + editor.getFormattedText(end, blockEnd)).trim();
            if (command.id === "duplicate") markdown += "\n\n" + markdown;
            else markdown = "\u200b";
            start = blockStart;
            end = blockEnd;
            block = true;
        }
        if (block) {
            const before = editor.getText(0, start);
            const lineStart = Math.max(before.lastIndexOf("\n"), before.lastIndexOf("\u2029")) + 1;
            if (before.slice(lineStart).trim()) markdown = "\n\n" + markdown;
        }
        editor.formatRange(start, end, markdown, block);
        if (selection) {
            const plain = editor.getText(0, editor.length);
            const offset = plain.indexOf(selection, start);
            if (offset >= 0) editor.select(offset, offset + selection.length);
            editor.continuation = null;
        }
        save();
        focusContent();
    }
    Settings {
        id: saved
        location: "file://" + Quickshell.env("HOME") + "/.config/bingux/sidebar-notes.ini"
        property string note: ""
    }
    function normaliseHeadings(markdown) {
        return markdown.replace(/^(#{1,6})[ \t]+((?:\\#){1,6})[ \t]+/gm, (match, heading, literal) =>
            literal.replace(/\\/g, "") === heading ? heading + " " : match);
    }
    function focusContent() { editor.forceActiveFocus(); }
    function save() {
        saved.note = editor.text;
        saved.setValue("note", editor.text);
        saved.sync();
    }
    Component.onDestruction: save()
    Timer { id: saveDelay; interval: 350; onTriggered: root.save() }
    ScrollView {
        anchors.fill: parent
        anchors.bottomMargin: 24
        clip: true
        contentWidth: availableWidth
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        TextArea {
            id: editor
            objectName: "notesEditor"
            text: root.normaliseHeadings(saved.note)
            textFormat: TextEdit.MarkdownText
            wrapMode: TextEdit.Wrap
            selectByMouse: true
            color: Theme.text
            placeholderTextColor: Theme.muted
            selectionColor: Theme.textSelection
            selectedTextColor: Theme.text
            placeholderText: "Write a note… Type / for commands"
            font.family: Theme.fontFamily
            font.pixelSize: 16
            font.wordSpacing: 1
            padding: 12
            topPadding: 16
            bottomPadding: 16
            persistentSelection: true
            background: null
            DocumentSpacing { document: editor.textDocument }
            Accessible.name: "Sidebar notes"
            Accessible.description: slashMenu.visible
                ? "Note commands: " + (slashMenu.selectedCommand ? slashMenu.selectedCommand.title : "No matches") + ". Use arrow keys and Enter."
                : "Type slash for commands. Markdown shortcuts format your note as you type."
            readonly property int activeBlockStart: {
                const before = getText(0, cursorPosition);
                return Math.max(before.lastIndexOf("\n"), before.lastIndexOf("\u2029")) + 1;
            }
            readonly property string activeSyntax: {
                const documentText = text;
                const tail = getText(activeBlockStart, length);
                const newline = tail.search(/[\n\u2029]/);
                const markdown = getFormattedText(activeBlockStart, newline < 0 ? length : activeBlockStart + newline).trim();
                const match = /^(#{1,6}|>|[-+*]|\d+\.|```)(?:\s|$)/.exec(markdown);
                return match ? match[1] : "";
            }
            Text {
                objectName: "notesSyntaxMarker"
                text: editor.activeSyntax
                parent: root
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 6
                z: 2
                color: Theme.muted
                opacity: editor.activeFocus && text.length > 0 ? 0.65 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.reducedMotion ? 0 : 100 } }
                font.family: Theme.fontFamily
                font.pixelSize: text.length > 4 ? 9 : 12
                Accessible.ignored: true
            }
            ContextMenu.menu: null
            ContextMenu.onRequested: position => root.showContextMenu(position)
            function formatBlock(style) {
                const plain = getText(0, length);
                const start = Math.max(plain.lastIndexOf("\n", Math.max(0, selectionStart - 1)), plain.lastIndexOf("\u2029", Math.max(0, selectionStart - 1))) + 1;
                const tail = plain.slice(Math.max(start, selectionEnd));
                const boundary = tail.search(/[\n\u2029]/);
                const end = boundary < 0 ? length : selectionEnd + boundary;
                let markdown = getFormattedText(Math.max(0, start - 1), end).trim();
                const heading = /^heading([1-6])$/.exec(style);
                if (style === "code") markdown = "```\n" + getText(start, end) + "\n```";
                else if (style === "plain") markdown = getText(start, end).replace(/([\\`*_{}\[\]()#+.!>|~-])/g, "\\$1");
                else {
                    let number = 0;
                    markdown = markdown.split("\n").map(line => {
                        if (!line.trim()) return line;
                        const text = line.replace(/^(?:#{1,6}\s+|>\s?|[-+*]\s+|\d+\.\s+)/, "");
                        return (heading ? "#".repeat(Number(heading[1])) + " " : style === "heading" ? "## " : style === "bullet" ? "- " : style === "numbered" ? (++number) + ". " : "> ") + text;
                    }).join("\n");
                }
                formatRange(start, end, markdown || "\u200b", true);
            }
            function finishMarkdownLink() {
                const start = activeBlockStart;
                if (/^\s*```/.test(getFormattedText(start, cursorPosition))) return false;
                const tail = getText(start, length);
                const boundary = tail.search(/[\n\u2029]/);
                const line = boundary < 0 ? tail : tail.slice(0, boundary);
                const pattern = /!?\[[^\]\n]+\]\([^\s)]+\)/g;
                let match;
                while ((match = pattern.exec(line)) !== null) {
                    const from = start + match.index, to = from + match[0].length;
                    if (cursorPosition >= from && cursorPosition <= to) {
                        formatRange(from, to, match[0], false);
                        return true;
                    }
                }
                return false;
            }
            property bool formatting: false
            property var continuation: null

            // Insert a formatted fragment, preserving the rest of the document
            // and its native cursor, selection, and undo history.
            function formatRange(start, end, markdown, emptyBlock) {
                const undoGroup = {before: text, states: [], steps: 0, after: ""};
                formatting = true;
                let continuationFont = Qt.font({
                    family: cursorSelection.font.family,
                    pointSize: cursorSelection.font.pointSize,
                    bold: cursorSelection.font.bold,
                    italic: cursorSelection.font.italic,
                    underline: cursorSelection.font.underline,
                    strikeout: cursorSelection.font.strikeout
                });
                // Qt merges the first imported block into the current block.
                // Anchor block edits at the document start: importing a middle
                // heading as the first fragment block loses its heading level.
                if (emptyBlock && start > 0) {
                    markdown = getFormattedText(0, start - 1).replace(/\s+$/, "") + "\n\n" + markdown;
                    start = 0;
                }
                if (end > start) {
                    remove(start, end);
                    undoGroup.states.push(text);
                    undoGroup.steps++;
                }
                const before = length;
                insert(start, markdown);
                undoGroup.states.push(text);
                undoGroup.steps++;
                const added = length - before;
                cursorPosition = start + added;
                if (emptyBlock) {
                    const marker = getText(start, start + added).indexOf("\u200b");
                    if (marker >= 0) {
                        // Removing the placeholder drops its character format.
                        // Carry that format into the first character the user types.
                        select(start + marker, start + marker + 1);
                        continuationFont = Qt.font({
                            family: cursorSelection.font.family,
                            pointSize: cursorSelection.font.pointSize,
                            bold: cursorSelection.font.bold,
                            italic: cursorSelection.font.italic,
                            underline: cursorSelection.font.underline,
                            strikeout: cursorSelection.font.strikeout
                        });
                        remove(start + marker, start + marker + 1);
                        undoGroup.states.push(text);
                        undoGroup.steps++;
                        cursorPosition = start + marker;
                    }
                }
                continuation = { position: cursorPosition, length: length, prefix: getText(0, cursorPosition), font: continuationFont };
                formatting = false;
                undoGroup.after = text;
                root.formatUndoGroups = root.formatUndoGroups.concat([undoGroup]).slice(-32);
            }
            function formatShortcut() {
                if (formatting || inputMethodComposing || selectionStart !== selectionEnd)
                    return;
                const end = cursorPosition;
                if (continuation) {
                    const pending = continuation;
                    continuation = null;
                    const added = length - pending.length;
                    if (added > 0 && end === pending.position + added && getText(0, pending.position) === pending.prefix) {
                        select(pending.position, end);
                        cursorSelection.font = pending.font;
                        cursorPosition = end;
                    }
                }
                const before = getText(0, end);
                const start = Math.max(before.lastIndexOf("\n"), before.lastIndexOf("\u2029")) + 1;
                const line = before.slice(start);
                // Code is literal: typing Markdown inside it must not format it.
                if (/^\s*```/.test(getFormattedText(start, end)))
                    return;
                const existingHeading = /^(#{1,6})\s/.exec(getFormattedText(start, end));
                if (/^#{1,6} $/.test(line) && existingHeading) {
                    formatting = true;
                    remove(start, end);
                    cursorPosition = start;
                    formatting = false;
                    formatBlock("heading" + line.trim().length);
                    return;
                }
                if (/^(#{1,6}|[-+*]|\d+\.|>) $/.test(line)) {
                    formatRange(start, end, line + "\u200b", true);
                    return;
                }
                if (line === "``` ") {
                    formatRange(start, end, "```\n\u200b\n```", true);
                    return;
                }
                const patterns = [
                    /\*\*[^*\n]+\*\*$/,
                    /__[^_\n]+__$/,
                    /~~[^~\n]+~~$/,
                    /`[^`\n]+`$/,
                    /\[[^\]\n]+\]\([^\s)]+\)$/,
                    /(?:^|[^*])(\*[^*\n]+\*)$/,
                    /(?:^|[^_])(_[^_\n]+_)$/
                ];
                for (const pattern of patterns) {
                    const match = pattern.exec(line);
                    if (!match)
                        continue;
                    const fragment = match[1] || match[0];
                    formatRange(end - fragment.length, end, fragment, false);
                    return;
                }
            }
            onTextEdited: {
                formatShortcut();
                slashMenu.update(true);
                root.save();
            }
            onTextChanged: saveDelay.restart()
            onCursorPositionChanged: Qt.callLater(() => slashMenu.update(false))
            onActiveFocusChanged: if (!activeFocus) slashMenu.close()
            onInputMethodComposingChanged: if (inputMethodComposing) slashMenu.close()
            Keys.onPressed: event => {
                if (slashMenu.handleKey(event)) { event.accepted = true; return; }
                if (event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && (event.modifiers & Qt.ShiftModifier))) {
                    root.showContextMenu(Qt.point(cursorRectangle.x, cursorRectangle.y + cursorRectangle.height));
                    event.accepted = true;
                    return;
                }
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    if (finishMarkdownLink()) {
                        root.save();
                        event.accepted = true;
                        return;
                    }
                    const before = getText(0, cursorPosition);
                    const start = Math.max(before.lastIndexOf("\n"), before.lastIndexOf("\u2029")) + 1;
                    const blockMarkdown = getFormattedText(Math.max(0, start - 1), cursorPosition).trim();
                    if (!(event.modifiers & Qt.ShiftModifier) && /^#{1,6} /.test(blockMarkdown)) {
                        formatRange(start, cursorPosition, blockMarkdown + "\n\n\u200b", true);
                        root.save();
                        event.accepted = true;
                        return;
                    }
                    if (before.slice(start) === "```") {
                        formatRange(start, cursorPosition, "```\n\u200b\n```", true);
                        root.save();
                        event.accepted = true;
                        return;
                    }
                }
                if (event.modifiers & Qt.ControlModifier) {
                    if (event.key === Qt.Key_Z || event.key === Qt.Key_Y) {
                        root.undoEdit(event.key === Qt.Key_Y || !!(event.modifiers & Qt.ShiftModifier));
                        event.accepted = true;
                    } else if (event.key === Qt.Key_B) {
                        continuation = null;
                        cursorSelection.font.bold = !cursorSelection.font.bold;
                        root.save();
                        event.accepted = true;
                    } else if (event.key === Qt.Key_I) {
                        continuation = null;
                        cursorSelection.font.italic = !cursorSelection.font.italic;
                        root.save();
                        event.accepted = true;
                    }
                }
            }
            onLinkActivated: link => Qt.openUrlExternally(link)
        }
    }
    Loader { id: contextMenuLoader }
    NotesSlashMenu { id: slashMenu; notes: root; editor: editor }

}
