### Bingux Preview.
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `preview-`.

# The app's name, in the window title, the header and the About window.
preview-title = Preview
# One line in the About window.
preview-description = View pictures and documents

## Tooltips of the header buttons

preview-open-tooltip = Open (Ctrl+O)
preview-info-tooltip = Info (I)
preview-pages-tooltip = Pages (F9)
preview-turn-left-tooltip = Turn left (Shift+R)
preview-turn-right-tooltip = Turn right (R)
preview-crop-tooltip = Crop (C)
preview-main-menu = Main menu

## The main menu

preview-menu-open = Open…
preview-menu-open-with = Open with
preview-menu-show-in-files = Show in Files
preview-menu-copy-picture = Copy picture
preview-menu-save-copy = Save a copy…
preview-menu-crop = Crop
preview-menu-turn-left = Turn left
preview-menu-turn-right = Turn right
preview-menu-flip-horizontally = Flip horizontally
preview-menu-flip-vertically = Flip vertically
preview-menu-fit = Fit to window
preview-menu-actual-size = Actual size
preview-menu-slideshow = Slideshow
preview-menu-stop-slideshow = Stop slideshow
preview-menu-full-screen = Full screen
preview-menu-leave-full-screen = Leave full screen
preview-menu-trash = Move to Trash
preview-menu-undo = Undo
preview-menu-shortcuts = Keyboard shortcuts

## The empty window

preview-empty-title = No file open
preview-empty-line = Drop a picture or document here.
preview-empty-action = Open file
preview-drop-title = Drop to open

## The file choosers

preview-open-request-title = Open
preview-open-filter = Pictures and documents
preview-save-request-title = Save a copy

## Toasts

preview-copied = Picture copied
preview-copy-failed = Could not copy the picture. Install wl-clipboard or xclip, then try again.
preview-save-failed = Could not save the copy. Check that the folder can be written to, then try again.
preview-crop-failed = Could not open the picture to crop it. Try another picture.
preview-crop-undone = Cropped copy moved to the Trash
preview-undo-copy-failed = Could not remove the copy. Move it to the Trash from Files.
preview-undo-failed = Could not put the file back. Open the Trash in Files to restore it.
preview-show-in-files-failed = Could not show the file. Open its folder in Files.

# One file's name, in typographic quotes. The name is the file's own.
# $name (Text) - the file's name
preview-moved-to-trash = “{ $name }” moved to the Trash
# $name (Text) - the file's name
preview-trash-failed = Could not move “{ $name }” to the Trash. Check that you can change its folder.
# $name (Text) - the name of the copy saved
preview-saved-copy = Copy saved as “{ $name }”
# $name (Text) - the name of the cropped copy saved
preview-saved-crop = Cropped copy saved as “{ $name }”
# $app (Text) - the name of the application that did not start
preview-app-failed = Could not start “{ $app }”. Check that it is still installed, then try again.

## Buttons and labels of the crop

preview-crop-area = Crop area
preview-crop-save = Save a copy
preview-cancel = Cancel
preview-close = Close
preview-undo = Undo

# The position of the picture in its folder, in the title line.
# $at (Number) - the position of the picture, from 1
# $of (Number) - how many pictures are in the folder
preview-position = { $at } of { $of }

# The picture a screen reader names for the stage.
# $name (Text) - the picture's file name
preview-picture-label = Picture: { $name }

## The keyboard shortcuts sheet

preview-shortcuts =
    Ctrl+O  Open
    Left, Right  Previous and next file in the folder
    Space  Play or pause an animated picture, page down in a document
    +, -, 0, 1  Zoom in, zoom out, fit the window, actual size
    R, Shift+R  Turn right, turn left
    H, V  Flip horizontally, vertically
    Ctrl and wheel  Zoom at the pointer
    Shift and an arrow  Move a zoomed picture
    Ctrl+C  Copy the picture, or the selected text of a document
    Ctrl+Shift+S  Save a copy of a picture
    C  Crop a picture and save the result as a copy
    Delete  Move a picture to the Trash
    Ctrl+Z  Put the last file moved to the Trash back, or remove the last cropped copy
    S  Start or stop a slideshow of the folder
    I  Show or hide the information
    F9  Show or hide the pages of a document
    Ctrl+F  Find in a document
    F, F11  Full screen
    Escape  Leave full screen or the slideshow
