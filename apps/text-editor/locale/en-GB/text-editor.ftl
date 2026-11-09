### Bingux Text Editor.
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `text-editor-`.

# The app's name, in the window title, the header and the About window.
text-editor-title = Text Editor
# One line in the About window.
text-editor-description = Edit text files and code

## Common

text-editor-ok = OK
text-editor-none = None

## Statistics

text-editor-stats-title = Statistics of “{ $title }”
text-editor-stats-words = Words: { $count }
text-editor-stats-characters = Characters: { $count }
text-editor-stats-characters-no-spaces = Characters without spaces: { $count }
text-editor-stats-lines = Lines: { $count }
text-editor-stats-paragraphs = Paragraphs: { $count }
text-editor-stats-reading-time = Reading time: { $time }
text-editor-stats-size = Size: { $size }
text-editor-stats-selected = Selected: { $words } words, { $characters } characters
text-editor-minutes = { $count ->
    [one] 1 minute
   *[other] { $count } minutes
}
text-editor-size-bytes = { $count ->
    [one] 1 byte
   *[other] { $count } bytes
}
text-editor-size-kilobytes = { $size } kB
text-editor-size-megabytes = { $size } MB

## Opening a file

text-editor-opening = Opening “{ $name }”…
text-editor-opening-size = Opening “{ $name }” ({ $size })…
text-editor-open-failed-title = Could not open this file
text-editor-try-again = Try again
text-editor-binary-title = This file is not text
text-editor-binary-description = It holds data that is not text. Saving it here can damage it.
text-editor-open-anyway = Open anyway

## Closing with unsaved changes

text-editor-close-title = Save changes to “{ $name }”?
text-editor-close-description = If you do not save, your changes are lost.
text-editor-discard = Discard
text-editor-cancel = Cancel
text-editor-save = Save
text-editor-link-not-web = Only web and mail links open from the preview

## Recent files

text-editor-no-recent = No recent files
text-editor-clear-recent = Clear recent files

## Notice banners

text-editor-reloaded = “{ $name }” changed on disk and was read again.
text-editor-undo = Undo
text-editor-changed-on-disk = “{ $name }” changed on disk. Your changes are not saved.
text-editor-read-again = Read again
text-editor-keep-mine = Keep mine
text-editor-deleted = “{ $name }” was moved or deleted.
text-editor-read-only = You can read this file but cannot save changes to it.
text-editor-save-as = Save as…
text-editor-mixed-encoding = This file mixes UTF-8 with { $encoding } text. Saving writes it all as UTF-8.
text-editor-lossy = Some characters do not fit this encoding and show as �. Saving replaces them.
text-editor-mixed-endings = This file mixes line endings. Saving writes one kind.

## Commands

text-editor-cmd-new = New document
text-editor-cmd-open = Open…
text-editor-cmd-open-recent = Open recent
text-editor-cmd-save = Save
text-editor-cmd-save-as = Save as…
text-editor-cmd-save-all = Save all
text-editor-cmd-reload = Read again from disk
text-editor-cmd-find = Find…
text-editor-cmd-replace = Find and replace…
text-editor-cmd-find-next = Find next
text-editor-cmd-find-previous = Find previous
text-editor-cmd-go-line = Go to line…
text-editor-cmd-line-numbers = Line numbers
text-editor-cmd-wrap = Wrap lines
text-editor-cmd-whitespace = Show spaces and tabs
text-editor-cmd-highlight-line = Highlight current line
text-editor-cmd-markdown-preview = Markdown preview
text-editor-cmd-zoom-in = Zoom in
text-editor-cmd-zoom-out = Zoom out
text-editor-cmd-actual-size = Actual size
text-editor-cmd-statistics = Statistics
text-editor-cmd-auto-close = Close brackets and quotes
text-editor-cmd-trim-on-save = Trim spaces on save
text-editor-cmd-final-newline = Add final new line

## Dialogs, menus and placeholders

text-editor-chooser-open = Open
text-editor-chooser-save-as = Save as
text-editor-filter-text = Text files
text-editor-filter-all = All files
text-editor-replaced = Replaced { $count ->
    [one] { $count } match
   *[other] { $count } matches
}
text-editor-change-case = Change case
text-editor-drop-hint = Start typing or drop a file
text-editor-drop-to-open = Drop to open
text-editor-scope-editor = Editor
text-editor-show-text = Show text
text-editor-show-preview = Show preview
text-editor-document-menu = Document
text-editor-other-languages = Other languages
text-editor-tab-width = Tab width
text-editor-detect-indent = Detect indentation
text-editor-encode-as = Encode as
text-editor-read-again-as = Read again as
text-editor-bom = Byte order mark
text-editor-group-editing = Editing
