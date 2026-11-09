### bingux-ui: the strings of the Bingux app SDK itself (dialogs, menus,
### the edit bar, window controls, the About window, the file chooser).
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `ui-`.

-brand-name = Bingux

## Text editing: the right-click menu of a field and the edit bar a touch
## or a long press shows over a selection.

ui-edit-cut = Cut
ui-edit-copy = Copy
ui-edit-paste = Paste
# Selects the word at the caret.
ui-edit-select = Select
ui-edit-select-all = Select all

## Buttons inside a text field

# Tooltip of the button that empties a search or text field.
ui-field-clear = Clear
ui-field-show-password = Show password
ui-field-hide-password = Hide password

## Dialogs

ui-dialog-cancel = Cancel

## Tabs: the tab strip's menu and the app's tab commands

ui-tab-close = Close tab
ui-tab-close-others = Close other tabs
# The tabs before this one in reading order: to the left in a left-to-right
# language, to the right in a right-to-left one. Translate for your reading
# direction.
ui-tab-close-to-start = Close tabs to the left
# The tabs after this one in reading order (see ui-tab-close-to-start).
ui-tab-close-to-end = Close tabs to the right
ui-tab-duplicate = Duplicate tab
ui-tab-reopen = Reopen closed tab
# Moves the tab one place towards the start of the strip (left in a
# left-to-right language, right in a right-to-left one).
ui-tab-move-to-start = Move left
# Moves the tab one place towards the end of the strip.
ui-tab-move-to-end = Move right

## Closing a window that would lose work. The title and message come from
## the app.

ui-window-keep-open = Keep open
ui-window-close = Close window

## A part of the window that failed. The program's own message is never
## shown; Copy details puts it on the clipboard for a bug report.

# Title of the card shown in place of a part that failed to draw.
# $name (String) - the part or the app, such as "Files" or "Sidebar"
ui-error-title = Could not show { $name }
ui-error-next-step = Try again. If it happens again, copy the details and report the problem.
ui-error-retry = Retry
ui-error-copy-details = Copy details
# Toast after a button, a task or a background job failed. The window
# carries on.
# $what (String) - the part that failed, or the app's name
ui-error-toast = { $what } stopped working. Copy the details and report the problem.
ui-error-toast-unnamed = Something stopped working. Copy the details and report the problem.
# $what (String) - the part that failed
ui-error-background = { $what } stopped working in the background.
# Toast in a window that was opened again after the app closed on a failure.
ui-error-restarted = The app closed after a problem and opened again.
ui-error-details-copied = Details copied
ui-error-details-not-copied = Could not copy the details. Try again.

## Window controls and the header bar

# The name of an app that did not give one, in "Could not show { $name }".
ui-app-unnamed = The app

ui-window-minimise = Minimise
ui-window-maximise = Maximise
ui-window-restore = Restore
# Tooltip and screen reader name of the window's close button.
ui-window-close-button = Close
# Tooltip of the spinner beside a window title while the app works.
ui-window-busy-status = Working…
ui-nav-back = Back
ui-nav-forward = Forward
# Tooltip of the close button of a banner or notice.
ui-notice-dismiss = Dismiss
# Tooltip of the close button of a dialog or sheet.
ui-dialog-close = Close

## The sidebar and its search

# Title of the page that lists what a sidebar search found.
ui-search-results = Search results
ui-search-no-matches = Nothing matches.
# Screen reader names of the sidebar and of its list.
ui-sidebar = Sidebar
ui-sidebar-pages = Pages
ui-sidebar-results = Results

## Forms

# Shown after the title of a field that may be left empty.
ui-form-optional = Optional
# Title of a group that shows the values in force now, beside a form.
ui-form-live-state = Right now
ui-form-unsaved = You have unsaved changes
ui-form-revert = Revert
ui-form-apply = Apply

## Field errors: what fits, never what is wrong. Addresses and keys in the
## examples are technical and stay as they are.

ui-validate-ipv4 = Enter an IPv4 address such as 192.168.1.10.
ui-validate-ipv6 = Enter an IPv6 address such as 2001:db8::1.
ui-validate-ip = Enter an IP address such as 192.168.1.10 or 2001:db8::1.
ui-validate-ipv4-cidr = Enter an address and prefix length such as 192.168.1.10/24 (0 to 32).
ui-validate-ipv6-cidr = Enter an address and prefix length such as 2001:db8::1/64 (0 to 128).
# $item (String) - the part of the list that is not an address
ui-validate-ip-list-item = Enter an IP address in place of “{ $item }”.
ui-validate-ip-list = Enter one or more IP addresses, separated by commas.
ui-validate-netmask-or-prefix = Enter a prefix length such as 24, or a netmask such as 255.255.255.0.
ui-validate-netmask = Enter a netmask with ones and then zeros, such as 255.255.255.0.
ui-validate-mac = Enter a MAC address such as 00:1A:2B:3C:4D:5E.
ui-validate-port = Enter a port number from 1 to 65535.
# $least (Number) - the smallest number allowed
# $most (Number) - the largest number allowed
ui-validate-integer = Enter a whole number from { NUMBER($least, useGrouping: "false") } to { NUMBER($most, useGrouping: "false") }.
ui-validate-host = Enter a host name such as vpn.example.com, or an IP address.
ui-validate-url = Enter a full address such as http://example.com/proxy.pac.
ui-validate-wireguard-key = Enter a WireGuard key: 44 letters, digits, + and /, ending in =.
ui-validate-ssid-missing = Enter the network name.
# A Wi-Fi network name is limited to 32 bytes; a letter with an accent takes two.
# $bytes (Number) - how many bytes the typed name takes
ui-validate-ssid-long = Use a shorter name. A network name fits in 32 bytes, and this one takes { $bytes }.
ui-validate-wpa-hex = Use only the digits 0 to 9 and the letters A to F in a 64-character key.
ui-validate-wpa-short = Use at least 8 characters.
ui-validate-wpa-long = Use at most 63 characters, or 64 for a hexadecimal key.
ui-validate-wpa-characters = Use only letters, digits, spaces and punctuation without accents.

## Settings commands that failed. $feature is one of the names below.

# What a system tool does, in the words of the person using Settings. Used
# inside the sentences after it, so lower case unless it is a name.
# $program (String) - the tool, such as nmcli
ui-command-feature = { $program ->
    [nmcli] network settings
    [wpctl] sound settings
    [bluetoothctl] Bluetooth settings
    [gsettings] desktop settings
    [timedatectl] date and time settings
    [upower] battery information
    [powerprofilesctl] power mode settings
    [gnoblinctl] display and input settings
    [busctl] account and display settings
    [bingux-config] Bingux settings
    [gio] application settings
   *[other] { $program }
}
# $feature (String) - from ui-command-feature
ui-command-missing = Could not load { $feature }. A part it needs is not installed.
# $feature (String) - from ui-command-feature
ui-command-not-started = Could not load { $feature }. Try again.
# $feature (String) - from ui-command-feature
ui-command-timeout = No answer from { $feature }. Try again in a moment.
# $feature (String) - from ui-command-feature
ui-command-failed = Could not change { $feature }. Try again.

## The About window of every app

# Title of the About window.
# $app (String) - the app's name, such as "Files"
ui-about-title = About { $app }
# Title of the About window's first page.
ui-about-page-main = About
# $version (String) - such as 0.1.0
ui-about-version = Version { $version }
# $version (String) - such as 0.1.0
# $commit (String) - the first characters of the source revision, such as 444fb3b
ui-about-version-commit = Version { $version } ({ $commit })
ui-about-website = Website
ui-about-report-issue = Report an issue
ui-about-link-failed = Could not open the link. Try again.
ui-about-licence = Licence
ui-about-licences = Licences
ui-about-credits = Credits
ui-about-legal = Legal
ui-about-details = Details
ui-about-app-id = App ID
# The source revision and date the app was built from.
ui-about-build = Build
ui-about-written-by = Written by
ui-about-built-with = Built with
# The library every Bingux app's interface is made with (bingux-ui).
ui-about-interface-library = Interface library
ui-about-libraries = Libraries
ui-about-licences-each = Licences of each library
# $app (String) - the app's name
ui-about-no-licence = { $app } does not say which licence it is under.
# $app (String) - the app's name
# $licence (String) - a licence identifier such as GPL-3.0-or-later
ui-about-licence-without-text = { $app } is licensed under { $licence }. It did not include the text of the licence.
# $app (String) - the app's name
ui-about-no-authors = { $app } does not list its authors.
# $app (String) - the app's name
ui-about-no-library-licences = The licences of the libraries { $app } is built from are not listed in this build.
# $path (String) - a folder on the computer
ui-about-licence-files = The full licence texts are in { $path }.

## Commands: the menu groups, the command palette and the keyboard
## shortcuts sheet every window has

ui-command-group-app = General
ui-command-group-file = File
ui-command-group-edit = Edit
ui-command-group-view = View
# The group of commands that move between pages, folders and tabs.
ui-command-group-go = Go
ui-command-group-window = Window
ui-command-group-help = Help
# .keywords: words that also find the command in the palette, separated by
# commas. Translate them as search words, not as a sentence.
ui-command-palette = Command palette
    .keywords = commands, actions, find
ui-command-shortcuts = Keyboard shortcuts
    .keywords = keys, help
# The About command; its title is ui-about-title.
ui-command-about =
    .keywords = version, licence, credits
# Title of the command palette dialog.
ui-palette-title = Commands
ui-palette-search =
    .placeholder = Search commands
    .a11y = Search commands
ui-palette-list =
    .a11y = Commands
# Read out by a screen reader as the person types.
# $count (Number) - how many commands match the search
ui-palette-matches-status = { $count ->
    [0] No commands match
    [one] { $count } command matches
   *[other] { $count } commands match
}
# $query (String) - what the person typed
ui-palette-no-match-empty = No commands match “{ $query }”
ui-shortcuts-title = Keyboard shortcuts
ui-shortcuts-search =
    .placeholder = Search shortcuts
ui-shortcuts-none-empty = No keyboard shortcuts in this window
# $query (String) - what the person typed
ui-shortcuts-no-match-empty = No shortcuts match “{ $query }”

## Key names on key caps and in shortcut labels. Use the names printed on
## keyboards sold for the language (German: Strg, Umschalt, Entf).

ui-key-ctrl = Ctrl
ui-key-alt = Alt
ui-key-shift = Shift
# The key with the system's logo.
ui-key-super = Super
ui-key-enter = Enter
ui-key-escape = Esc
ui-key-tab = Tab
ui-key-backspace = Backspace
ui-key-delete = Delete
ui-key-insert = Insert
ui-key-home = Home
ui-key-end = End
ui-key-page-up = Page Up
ui-key-page-down = Page Down
# The arrow keys.
ui-key-up = Up
ui-key-down = Down
ui-key-left = Left
ui-key-right = Right
# The key that opens a context menu.
ui-key-menu = Menu
ui-key-plus = Plus
ui-key-minus = Minus
ui-key-space = Space

## Undo and Redo. $action is the change's name as the app gave it, first
## letter in lower case, such as "rename" or "move to Trash".

ui-undo = Undo
ui-redo = Redo
# $action (String) - the change, such as "rename"
ui-undo-named = Undo { $action }
# $action (String) - the change
ui-redo-named = Redo { $action }
# Read out by a screen reader after an undo.
ui-undo-done-status = Undone
ui-redo-done-status = Redone
# $action (String) - the change that was undone
ui-undo-named-done-status = Undid { $action }
# $action (String) - the change that was redone
ui-redo-named-done-status = Redid { $action }

## Copying text

# Toast after text was copied.
ui-copy-done-status = Copied
ui-copy-failed-error = Could not copy the text. Try again.
# Right-click menu of a file path: copies the whole path.
ui-path-copy-path = Copy path
# Right-click menu of a file name.
ui-path-copy-name = Copy name

## The main menu of a window

# Tooltip and screen reader name of the menu button in a header bar.
ui-main-menu = Main menu

## Lists the person can reorder by dragging

# The name of an item with no title of its own.
ui-sortable-item = Item
# Read out after an item moved.
# $name (String) - the item
# $position (Number) - where it is now, counting from 1
# $count (Number) - how many items there are
ui-sortable-moved-status = { $name } now { $position } of { $count }
# The drag handle of an item with no title.
ui-sortable-grip =
    .a11y = Reorder
    .description = Press Up or Down to move, Home or End to send to an end.
# $name (String) - the item the handle moves
ui-sortable-grip-named =
    .a11y = Reorder { $name }

## Panes

ui-split-divider =
    .a11y = Pane divider

## Opening files and links with other apps

ui-open-no-app-error = No app is set up to open this. Install an app that opens this type of file.
ui-open-not-local-error = Could not open the file. Copy it to this computer, then open it again.
ui-open-gone-error = Could not open the file. Find where it was moved, then open it again.
ui-open-bad-address-error = Could not open the address. Check that it is typed correctly.
# $app (String) - the app's name
ui-open-app-not-started-error = Could not open { $app }. Try again.
ui-open-not-started-error = Could not open the app. Try again.
ui-bus-missing-error = Could not reach the desktop. Log out and back in, then try again.
ui-bus-timeout-error = The desktop did not answer. Try again in a moment.

## Charts

ui-chart-no-data-empty = No data yet
# Screen reader names of charts that have no title of their own.
ui-chart-sparkline = Sparkline
ui-chart-line = Line chart
ui-chart-bars = Bar chart
ui-chart-gauge = Gauge
ui-chart-donut = Donut chart
ui-chart-flame = Flame graph
ui-chart-timeline = Timeline chart
# Title of the tooltip over a point of a sparkline.
# $position (Number) - the point, counting from 1
# $count (Number) - how many points there are
ui-chart-point = Point { $position } of { $count }
# The name of a series with no name of its own.
ui-chart-value = Value
# The sum of a stacked bar.
ui-chart-total = Total
# Short durations on a chart's axis. Use the language's unit abbreviations.
# $days (Number)
# $hours (Number)
ui-chart-days-hours = { $hours ->
    [0] { $days } d
   *[other] { $days } d { $hours } h
}
# $hours (Number)
# $minutes (Number)
ui-chart-hours-minutes = { $minutes ->
    [0] { $hours } h
   *[other] { $hours } h { $minutes } min
}
# $minutes (Number)
ui-chart-minutes = { $minutes } min
# $seconds (Number)
ui-chart-seconds = { $seconds } s
# The line under a flame graph while the pointer is on a bar.
# $share (Number) - percentage of all samples in this call, one decimal
# $own (Number) - percentage spent in the call itself, one decimal
# $value (String) - the call's total, already formatted
# $depth (Number) - how many calls deep the bar is
ui-flame-bar-description = { $share }% of all  ·  { $own }% in the call itself  ·  { $value }  ·  depth { $depth }
ui-flame-hint = Point at a bar for its full name and numbers. Click it to zoom in, and press Escape to zoom out.
# $name (String) - the call zoomed to
ui-flame-zoomed-hint = Zoomed to { $name }. Press Escape to zoom out, or point at a bar for its full name and numbers.
# $depth (Number) - how many calls deep the view starts
ui-flame-split-hint = Showing from { $depth } calls down, where the work splits. Point at a bar for its full name and numbers, or click it to zoom in.
# Button that zooms back out to the whole graph.
ui-flame-all-calls = All calls
ui-flame-none-empty = Nothing sampled in this range

## The colour picker

ui-colour-title = Colour
ui-colour-done = Done
ui-colour-hex =
    .a11y = Hex colour
ui-colour-opacity = Opacity
ui-colour-red = Red
ui-colour-green = Green
ui-colour-blue = Blue
ui-colour-hue = Hue
ui-colour-saturation = Saturation
ui-colour-lightness = Lightness
# The V of HSV: how bright the colour is.
ui-colour-value = Value
ui-colour-recent = Recent
ui-colour-picker =
    .a11y = Colour picker
ui-colour-plane =
    .a11y = Saturation and brightness
# $saturation (Number) - 0 to 100
# $brightness (Number) - 0 to 100
ui-colour-plane-value =
    .a11y = Saturation { $saturation }%, brightness { $brightness }%
# $old (String) - the colour before, as #rrggbb
# $new (String) - the colour now
ui-colour-preview =
    .a11y = Colour, was { $old }, now { $new }
# $colour (String) - the colour before, as #rrggbb
ui-colour-old =
    .a11y = Restore old colour { $colour }
# A named swatch, such as "Blue, #3584e4".
# $name (String) - the colour's name
# $hex (String) - the colour as #rrggbb
ui-colour-named = { $name }, { $hex }
# $colour (String) - the swatch's name and hex value
ui-colour-swatch =
    .a11y = Colour { $colour }
ui-colour-hex-missing-error = Enter a hex colour, such as #3584e4.
# $digit (String) - the character that is not 0-9 or a-f
ui-colour-hex-digit-error = “{ $digit }” is not a hex digit. Use 0 to 9 and A to F.
# $count (Number) - how many digits were typed
ui-colour-hex-length-error = { $count ->
    [one] { $count } digit. Use 3, 4, 6 or 8.
   *[other] { $count } digits. Use 3, 4, 6 or 8.
}

## File previews (Quick Look)

ui-preview-title = Preview
ui-preview-nothing = Nothing to preview
ui-preview-none-empty = No preview for this file
ui-preview-open =
    .tooltip = Open (Enter)
ui-preview-info =
    .tooltip = Info (I)
ui-preview-popup =
    .tooltip = Open as a popup (Alt+P)
ui-preview-dock =
    .tooltip = Dock to the side (Alt+P)
ui-preview-hide =
    .tooltip = Hide the preview
ui-preview-close =
    .tooltip = Close (Space)
ui-preview-properties = Properties
ui-preview-edge =
    .a11y = Preview pane edge
ui-preview-not-started-error = Could not start the preview. Try again.
ui-preview-stopped-error = This preview was stopped to make room for another. Open it again to see it.
# The names in the Info panel of a preview.
ui-info-name = Name
ui-info-type = Type
ui-info-size = Size
ui-info-modified = Modified
# The folder the file is in.
ui-info-location = Location
ui-info-dimensions = Dimensions
ui-info-resolution = Resolution
ui-info-camera = Camera
ui-info-lens = Lens
# When a photo was taken.
ui-info-taken = Taken
ui-info-exposure = Exposure
ui-info-software = Software
# Where a photo was taken.
ui-info-place = Place
# How long a song or a video plays.
ui-info-length = Length
ui-info-codecs = Codecs
# The character encoding of a text file.
ui-info-encoding = Encoding
ui-info-format = Format
# What an archive holds.
ui-info-contents = Contents
# $count (Number) - how many files the archive holds
ui-info-files = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
# The archive was too large to count every file.
# $count (Number) - how many files were counted
ui-info-files-at-least = { $count ->
    [one] { $count } file or more
   *[other] { $count } files or more
}
# The size of an archive's files once unpacked.
ui-info-unpacked = Unpacked
ui-info-family = Family
ui-info-style = Style
ui-info-version = Version
ui-info-designer = Designer
ui-info-glyphs = Glyphs

## Files: views and states

## What a folder shows when there is no list: loading, empty, stopped, failed.

# Title of the page when a folder has nothing to show.
ui-files-state-recent-title = No recent files
ui-files-state-recent-description = Files you open appear here.
ui-files-state-filtered-title = No matching files
ui-files-state-filtered-description = There are no files of the chosen type in this folder. Choose another type, or open another folder.
ui-files-state-hidden-title = No files to show
ui-files-state-hidden-description = This folder only has hidden files. Choose Show hidden files in the view menu to see them.
ui-files-state-empty-title = Folder is empty
ui-files-state-empty-description = There is nothing in this folder.
ui-files-state-stopped-title = Loading stopped
ui-files-state-stopped-description = Press F5 to read the folder again.
ui-files-state-trash-title = Trash is empty
ui-files-state-trash-description = Things you move to the Trash appear here until you empty it.

## A folder that could not be read. The title says what failed.

ui-files-error-permission-title = No permission
ui-files-error-not-found-title = Folder not found
ui-files-error-not-folder-title = Not a folder
ui-files-error-unresponsive-title = Drive not responding
ui-files-error-open-title = Cannot open folder

## Search with no results.

ui-files-search-invalid-title = Cannot search
ui-files-search-searching-title = Searching
ui-files-search-searching-description = Nothing has matched yet.
ui-files-search-none-title = Nothing found
# $place (String) - the folder searched, such as "Home"
ui-files-search-none-all-description = Nothing in { $place } fits these filters.
# $place (String) - the folder searched, such as "Home"
# $words (String) - what the person typed
ui-files-search-none-words-description = No name in { $place } matches “{ $words }”.

## The status line under a folder: what is on show, selected and free.

ui-files-cancel = Cancel
ui-files-status-unresponsive = Waiting for the drive to answer…
ui-files-status-stopped = Stopped
# $count (Number) - how many items have come in so far
ui-files-status-stopped-so-far = { $count ->
    [one] Stopped · { NUMBER($count) } item so far
   *[other] Stopped · { NUMBER($count) } items so far
}
ui-files-status-loading = Loading…
# $count (Number) - how many items have come in so far
ui-files-status-loading-so-far = { $count ->
    [one] Loading · { NUMBER($count) } item so far
   *[other] Loading · { NUMBER($count) } items so far
}
# $count (Number) - how many items the folder shows
ui-files-status-items = { $count ->
    [one] { NUMBER($count) } item
   *[other] { NUMBER($count) } items
}
# $count (Number) - how many items the folder shows
# $free (String) - the free space on the drive, such as "4.2 GB"
ui-files-status-items-free = { $count ->
    [one] { NUMBER($count) } item, { $free } free
   *[other] { NUMBER($count) } items, { $free } free
}
# $count (Number) - how many items the Trash shows
ui-files-status-trash-items = { $count ->
    [one] Trash · { NUMBER($count) } item
   *[other] Trash · { NUMBER($count) } items
}
# $trash, $sized, $linked (String) - "yes" or "no", choosing the variant
# $selected (Number) - how many items are selected
# $shown (Number) - how many items the folder shows
# $size (String) - the size of the selection, such as "1.2 MB"
# $target (String) - where the one selected item links to
ui-files-status-selected = { $trash ->
    [yes] { $sized ->
        [yes] { $linked ->
            [yes] Trash · { NUMBER($selected) } of { NUMBER($shown) } selected ({ $size }) · Link to { $target }
           *[no] Trash · { NUMBER($selected) } of { NUMBER($shown) } selected ({ $size })
        }
       *[no] { $linked ->
            [yes] Trash · { NUMBER($selected) } of { NUMBER($shown) } selected · Link to { $target }
           *[no] Trash · { NUMBER($selected) } of { NUMBER($shown) } selected
        }
    }
   *[no] { $sized ->
        [yes] { $linked ->
            [yes] { NUMBER($selected) } of { NUMBER($shown) } selected ({ $size }) · Link to { $target }
           *[no] { NUMBER($selected) } of { NUMBER($shown) } selected ({ $size })
        }
       *[no] { $linked ->
            [yes] { NUMBER($selected) } of { NUMBER($shown) } selected · Link to { $target }
           *[no] { NUMBER($selected) } of { NUMBER($shown) } selected
        }
    }
}

## Files: column headings, dates and sorting.

ui-files-column-name = Name
ui-files-column-size = Size
ui-files-column-type = Type
ui-files-column-modified = Modified
ui-files-column-created = Created
ui-files-column-accessed = Accessed
ui-files-column-permissions = Permissions
ui-files-column-owner = Owner
ui-files-column-location = Location
ui-files-column-original-location = Original Location
ui-files-column-deleted = Deleted
# $column (String) - the column's heading, such as "Modified"
ui-files-column-sort-by = Sort by { $column }
# $column (String) - the column's heading, for a column that cannot be sorted
ui-files-column-heading-label = { $column } column
ui-files-columns-reset = Reset columns
ui-files-unknown = Unknown
ui-files-date-just-now = Just now
# $time (String) - the clock time, such as "14:05"
ui-files-date-today = Today { $time }
# $time (String) - the clock time, such as "14:05"
ui-files-date-yesterday = Yesterday { $time }

## Files: views, commands and the places they name.

ui-files-a11y-files = Files
ui-files-a11y-file-list = File list
ui-files-drag-copy = Copy
# $when (String) - when the item was deleted, such as "Today 14:05"
ui-files-deleted-when = Deleted { $when }
ui-files-root-computer = Computer
ui-files-root-home = Home
ui-files-root-recent = Recent
ui-files-root-trash = Trash
ui-files-view-list = List
ui-files-view-grid = Grid
# $when (String) - when the file was modified, such as "Today 14:05"
ui-files-hover-modified = Modified { $when }
ui-files-hover-modified-pending = Modified …
ui-files-hover-modified-unknown = Modified unknown
# $target (String) - the path a link points at
ui-files-hover-link-to = Link to { $target }
ui-files-cmd-new-folder = New folder
ui-files-cmd-duplicate = Duplicate
ui-files-cmd-rename = Rename…
ui-files-cmd-compress = Compress…
ui-files-cmd-extract-to = Extract to…
ui-files-cmd-move-to-trash = Move to Trash
ui-files-cmd-delete-permanently = Delete permanently…
ui-files-cmd-properties = Properties
ui-files-cmd-cut = Cut
ui-files-cmd-copy = Copy
ui-files-cmd-paste = Paste
ui-files-cmd-copy-path = Copy path
ui-files-cmd-select-all = Select all
ui-files-cmd-invert-selection = Invert selection
ui-files-cmd-preview = Preview
ui-files-cmd-select-by-name = Select an item by its name
ui-files-cmd-type-a-name = Type a name
ui-files-cmd-show-as-list = Show as list
ui-files-cmd-show-as-grid = Show as grid
ui-files-cmd-zoom = Larger, smaller, actual size
ui-files-cmd-sort-by = Sort by name, size, type, date
ui-files-cmd-reverse-order = Reverse the order
