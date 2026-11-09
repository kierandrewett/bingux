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
