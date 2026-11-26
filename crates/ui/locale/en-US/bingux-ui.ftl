### bingux-ui: the strings of the Bingux app SDK itself (dialogs, menus,
### the edit bar, window controls, the About window, the file chooser).
### Source language: American English. Conventions: docs/bingux-ui.md,
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

ui-window-minimise = Minimize
ui-window-maximise = Maximize
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
# The placeholder of every search field.
ui-search-placeholder = Search
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
ui-about-licence = License
ui-about-licences = Licenses
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
ui-about-licences-each = Licenses of each library
# $app (String) - the app's name
ui-about-no-licence = { $app } does not say which license it is under.
# $app (String) - the app's name
# $licence (String) - a license identifier such as GPL-3.0-or-later
ui-about-licence-without-text = { $app } is licensed under { $licence }. It did not include the text of the license.
# $app (String) - the app's name
ui-about-no-authors = { $app } does not list its authors.
# $app (String) - the app's name
ui-about-no-library-licences = The licenses of the libraries { $app } is built from are not listed in this build.
# $path (String) - a folder on the computer
ui-about-licence-files = The full license texts are in { $path }.

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
    .keywords = version, license, credits
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
# Between two keys that each run the command (Ctrl+/ or Ctrl+?).
ui-shortcut-or = or

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

## The color picker

ui-colour-title = Color
ui-colour-done = Done
ui-colour-hex =
    .a11y = Hex color
ui-colour-opacity = Opacity
ui-colour-red = Red
ui-colour-green = Green
ui-colour-blue = Blue
ui-colour-hue = Hue
ui-colour-saturation = Saturation
ui-colour-lightness = Lightness
# The V of HSV: how bright the color is.
ui-colour-value = Value
ui-colour-recent = Recent
ui-colour-picker =
    .a11y = Color picker
ui-colour-plane =
    .a11y = Saturation and brightness
# $saturation (Number) - 0 to 100
# $brightness (Number) - 0 to 100
ui-colour-plane-value =
    .a11y = Saturation { $saturation }%, brightness { $brightness }%
# $old (String) - the color before, as #rrggbb
# $new (String) - the color now
ui-colour-preview =
    .a11y = Color, was { $old }, now { $new }
# $colour (String) - the color before, as #rrggbb
ui-colour-old =
    .a11y = Restore old color { $colour }
# A named swatch, such as "Blue, #3584e4".
# $name (String) - the color's name
# $hex (String) - the color as #rrggbb
ui-colour-named = { $name }, { $hex }
# $colour (String) - the swatch's name and hex value
ui-colour-swatch =
    .a11y = Color { $colour }
ui-colour-hex-missing-error = Enter a hex color, such as #3584e4.
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
# The preview helper is the separate process that plays media and renders documents.
ui-preview-helper-missing = The preview helper (bingux-preview-host) is not installed.
# $error (Text) - the system's reason the helper did not start
ui-preview-helper-start-error = The preview helper did not start ({ $error }).
ui-preview-no-thread = The preview could not start (no thread).
ui-preview-timeout-open = The preview took too long to open and was stopped.
# $error (Text) - the reason the helper's file could not be opened
ui-preview-file-not-opened = The file could not be opened ({ $error }).
ui-preview-timeout = The preview took too long and was stopped.
# $status (Text) - the helper's exit status
ui-preview-helper-stopped-status = The preview helper stopped unexpectedly ({ $status }).
ui-preview-helper-stopped = The preview helper stopped unexpectedly.
# $error (Text) - the reason the helper's message could not be read
ui-preview-helper-unreadable = The preview helper sent something unreadable ({ $error }).
ui-preview-frame-invalid = The preview helper sent a frame that does not fit.
ui-preview-page-invalid = The preview helper sent a page that does not fit.
ui-preview-version-mismatch = The preview helper speaks another version.
ui-preview-open-button = Open
ui-preview-try-again-button = Try again
ui-preview-zoom-fit-width = Fit width
ui-preview-zoom-fit-page = Fit page
ui-preview-zoom-out-tooltip = Zoom out (-)
ui-preview-zoom-in-tooltip = Zoom in (+)
ui-preview-find-tooltip = Find (Ctrl+F)
# $pages (Number) - the document's page count, shown after the page number field
ui-preview-page-count = of { $pages }
ui-preview-previous-match-tooltip = Previous match (Shift+Enter)
ui-preview-next-match-tooltip = Next match (Enter)
ui-preview-close-find-tooltip = Close find (Escape)
ui-preview-find-placeholder = Find in document
ui-preview-untitled = Untitled
ui-preview-counting = Counting…
ui-preview-outline-tab = Outline
ui-preview-subtitles-tooltip = Subtitles (C)
ui-preview-playback-speed-tooltip = Playback speed ([ and ])

## Text in a preview
# $count (Number) - how many lines the text has
ui-preview-text-lines = { $count ->
    [one] { $count } line
   *[other] { $count } lines
}
ui-preview-text-capped = The preview stops here. Open the file to read the rest.
# $shown (Text) - how much of the file is shown, such as 64 KB
# $total (Text) - the file's size
ui-preview-text-partial = Showing { $shown } of { $total }.
ui-preview-text-show-more = Show more
ui-preview-text-reading = Reading…
# The menu of the text viewer's toolbar.
ui-preview-view-menu = View
ui-preview-text-wrap = Wrap lines
ui-preview-text-colour = Color code
ui-preview-text-larger = Larger text
ui-preview-text-smaller = Smaller text
ui-preview-text-default-size = Default text size
ui-preview-text-copied = Text copied
ui-preview-text-copy =
    .tooltip = Copy text

## Archives in a preview
# $count (Number) - how many files the archive holds
ui-preview-archive-files = { $count ->
    [one] { $count } file
   *[other] { $count } files
}
# $count (Number) - how many files were counted before the listing stopped
ui-preview-archive-files-or-more = { $count ->
    [one] { $count } file or more
   *[other] { $count } files or more
}
# $size (Text) - the size the files take when unpacked, such as 4.2 MB
ui-preview-archive-unpacked = { $size } unpacked
ui-preview-archive-extract-all = Extract all
# $name (Text) - the folder the archive was extracted into
ui-preview-archive-extracted = Extracted to “{ $name }”.
ui-preview-archive-show-in-files = Show in Files
ui-preview-archive-extract-failed = Could not extract the archive. Check that its folder can be written to and that the archive is not locked, then try again.

## Folders in a preview
ui-preview-folder-open-in-files = Open in Files
ui-preview-folder-open-failed = Could not open the folder. Try opening it from Files.

## Fonts in a preview
# $count (Number) - how many glyphs the font has
ui-preview-font-glyphs = { $count ->
    [one] { $count } glyph
   *[other] { $count } glyphs
}
# $count (Number) - how many fonts one file holds
ui-preview-font-faces = { $count } fonts in this file
# The sample line of a font. A sentence that uses every letter of the language, then the digits.
ui-preview-font-pangram = The quick brown fox jumps over the lazy dog. 0123456789
ui-preview-font-sample-placeholder = Type to try the font
ui-preview-font-install = Install
ui-preview-font-installed-button = Installed
# $family (Text) - the font's family name
ui-preview-font-installed = “{ $family }” is installed for you.
ui-preview-font-install-failed = Could not install the font. Check that your fonts folder can be written to, then try again.
# $count (Number) - how many characters the font draws
ui-preview-font-characters = { $count ->
    [one] { $count } character
   *[other] { $count } characters
}
ui-preview-font-character-copied = Character copied
ui-preview-font-copy-character =
    .tooltip = Copy character
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

## Files: properties, chooser and address bar

# $count (Number) - bytes
ui-files-props-bytes = { $count } bytes
# $size (String) - the size so far
ui-files-props-size-stopped = At least { $size } (stopped)
# $size (String) - the size so far
ui-files-props-size-so-far = { $size } so far…
# $size (String) - the size so far
ui-files-props-size-at-least = At least { $size }
# $count (Number) - items counted
ui-files-props-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
# $count (Number) - items counted
ui-files-props-items-stopped = { $count ->
    [one] At least { $count } item (stopped)
   *[other] At least { $count } items (stopped)
}
# $count (Number) - items counted
ui-files-props-items-so-far = { $count ->
    [one] { $count } item so far…
   *[other] { $count } items so far…
}
# $count (Number) - items counted
ui-files-props-items-at-least = { $count ->
    [one] At least { $count } item
   *[other] At least { $count } items
}
# $name (String) - kind name, $count (Number) - how many
ui-files-props-kind-count = { $name } ({ $count })
# $name (String) - the name, $id (String) - the numeric id
ui-files-props-named-id = { $name } ({ $id })
# $size (String) - the size on the drive
ui-files-props-on-disk = { $size } on disk
# $size (String), $count (Number) - folder size and item count
ui-files-props-folder-size = { $size }, { $count ->
    [one] { $count } item
   *[other] { $count } items
}
# $done (String), $total (String), $percent (String)
ui-files-props-read = { $done } of { $total } ({ $percent }%)
# $count (Number) - items changed
ui-files-props-changed = Changed { $count ->
    [one] { $count } item
   *[other] { $count } items
}
# $count (Number) - items changed, $failed (Number) - items that failed
ui-files-props-changed-failed = Changed { $count ->
    [one] { $count } item
   *[other] { $count } items
}; { $failed } could not be changed
# $reason (String) - why the file would not open
ui-files-chooser-open-failed = Could not open the file. { $reason }
# $count (Number) - files that would be replaced
ui-files-chooser-replace-title = { $count ->
    [one] Replace file?
   *[other] Replace files?
}
# $name (String) - the existing file's name
ui-files-chooser-replace-body-one = A file called “{ $name }” already exists in this folder. Replacing it overwrites its contents.
# $count (Number) - files that would be replaced
ui-files-chooser-replace-body = { $count } files with these names already exist in this folder. Replacing them overwrites their contents.
ui-files-chooser-cancel = Cancel
ui-files-chooser-replace = Replace
ui-files-chooser-back = Back
ui-files-chooser-forward = Forward
ui-files-chooser-parent = Parent folder
ui-files-chooser-home = Home
ui-files-chooser-go-to-location = Go to location…
    .keywords = path, address
ui-files-chooser-search = Search
ui-files-chooser-search-tooltip = Search (Ctrl+F)
ui-files-chooser-hidden = Show hidden files
    .keywords = dotfiles
ui-files-chooser-show-preview = Show preview pane
ui-files-chooser-preview-tooltip = Preview pane (Alt+P)
ui-files-chooser-reload = Reload
ui-files-chooser-partial = Loading stopped before every item was read. Reload to see them all.
ui-files-chooser-no-new-trash = Nothing can be made in the Trash
ui-files-chooser-new-folder = New folder
ui-files-chooser-empty-trash = Empty Trash
ui-files-chooser-show-list = Show a list
ui-files-chooser-show-grid = Show a grid
ui-files-chooser-view-options = View options
ui-files-chooser-show-as-list = Show as list
ui-files-chooser-show-as-grid = Show as grid
ui-files-chooser-main-menu = Main menu
ui-files-chooser-sort-name = Sort by name
ui-files-chooser-sort-size = Sort by size
ui-files-chooser-sort-type = Sort by type
ui-files-chooser-sort-modified = Sort by modified
ui-files-chooser-reverse-order = Reverse order
ui-files-chooser-file-name = File name
ui-files-chooser-file-type = File type
ui-files-omni-not-a-folder = That is a file, not a folder.
# $problem (String) - why the path cannot be used, $shown (String) - the path as typed
ui-files-omni-problem-shown = { $problem }: { $shown }
ui-files-omni-checking = Checking…
ui-files-omni-searching = Searching…
ui-files-omni-more-matches = More matches. Keep typing to narrow them.
ui-files-omni-folder-ellipsis = Folder…
ui-files-omni-in-this-folder = In this folder
ui-files-omni-places = Places
ui-files-omni-bookmark = Bookmark
ui-files-omni-recent-search = Recent search
# $search (String) - the search text
ui-files-omni-search-for = Search for { $search }
# $phrase (String) - the search text in the folder heading
ui-files-omni-search-in = Search in { $phrase }
# $folder (String) - the folder that cannot be searched
ui-files-omni-no-folder = There is no folder “{ $folder }” to look in
ui-files-omni-show-in-folder = Show in folder
ui-files-omni-no-results-filters = No results. Try fewer filters.
ui-files-omni-no-results = No results
ui-files-omni-placeholder = Type a path or search
ui-files-omni-label = Location and search
ui-files-omni-close-action = Show the path as folders
ui-files-places-title = Places
# $name (String) - the bookmark's name
ui-files-places-removed = Removed { $name } from the bookmarks.
ui-files-places-undo = Undo
# $error (String) - why the change failed
ui-files-places-change-failed = The bookmarks could not be changed: { $error }.
# $error (String) - why the rename failed
ui-files-places-rename-failed = The bookmark could not be renamed: { $error }.
ui-files-places-rename-title = Rename bookmark
ui-files-places-bookmark-name = Bookmark name
ui-files-places-cancel = Cancel
ui-files-places-rename = Rename
ui-files-places-copied = Copied.
ui-files-places-copy-failed = The path could not be copied.
ui-files-places-not-responding = Not responding
ui-files-address-location = Location
ui-files-address-remove-bookmark = Remove from bookmarks
ui-files-address-bookmark-remove-failed = The bookmark could not be removed.
ui-files-address-add-bookmark = Add to bookmarks
ui-files-address-bookmark-save-failed = The bookmark could not be saved.
ui-files-address-copy-location = Copy location
ui-files-address-copy-path = Copy path
ui-files-address-open = Open
ui-files-address-copied = Copied.
ui-files-address-copy-failed = The path could not be copied.
ui-files-address-hidden-folders = Hidden folders
# $title (String) - the folder whose subfolders are listed
ui-files-address-folders-in = Folders in { $title }
ui-files-address-loading = Loading…
ui-files-address-no-subfolders = No subfolders
ui-files-address-more-folders = More folders. Type the path to reach them.
ui-files-address-type-path = Type a path
ui-files-address-problem-not-found = Not found
ui-files-address-problem-not-a-folder = Not a folder
ui-files-address-problem-no-permission = No permission
ui-files-address-problem-not-responding = Not responding
ui-files-reload-stop = Stop (Esc)
ui-files-reload-refresh = Refresh (F5)
ui-files-launch-not-responding = not responding


## Files: actions and menus

# The menus of the Files app: the entry, empty-space and header menus.
ui-files-menu-open-with = Open with
ui-files-menu-paste-into-folder = Paste into folder
ui-files-menu-open = Open
ui-files-menu-open-in-new-tab = Open in new tab
ui-files-menu-preview = Preview
ui-files-menu-cut = Cut
ui-files-menu-copy = Copy
ui-files-menu-duplicate = Duplicate
ui-files-menu-restore = Restore
ui-files-menu-restore-to = Restore to…
ui-files-menu-rename = Rename
ui-files-menu-rename-many = Rename…
ui-files-menu-move-to-trash = Move to Trash
ui-files-menu-delete-permanently = Delete permanently
ui-files-menu-compress = Compress…
ui-files-menu-extract-here = Extract Here
ui-files-menu-extract-to = Extract to…
ui-files-menu-copy-path = Copy path
ui-files-menu-copy-uri = Copy location as URI
ui-files-menu-add-bookmark = Add to bookmarks
ui-files-menu-remove-bookmark = Remove from bookmarks
ui-files-menu-properties = Properties
ui-files-menu-compress-as = Compress as
ui-files-menu-quick-zip = Quick .zip
ui-files-menu-quick-targz = Quick .tar.gz
ui-files-menu-compress-selection = Compress selection…
ui-files-menu-sort-by = Sort by
ui-files-menu-list = List
ui-files-menu-grid = Grid
ui-files-menu-icon-size = Icon size
ui-files-menu-show-hidden = Show hidden files
ui-files-menu-new-file = New file
ui-files-menu-empty-file = Empty file
ui-files-menu-paste-into-trash = Paste into Trash
ui-files-menu-paste = Paste
ui-files-menu-new-folder = New folder
ui-files-menu-select-all = Select all
ui-files-menu-invert-selection = Invert selection
ui-files-menu-view = View
ui-files-menu-empty-trash = Empty Trash
ui-files-menu-refresh = Refresh

# The sort choices of the Sort by menu.
ui-files-sort-name = Name
ui-files-sort-size = Size
ui-files-sort-type = Type
ui-files-sort-modified = Modified
ui-files-sort-ascending = Ascending
ui-files-sort-descending = Descending
ui-files-sort-folders-first = Folders first

# The places sidebar menu: the rows for a bookmark, drive, computer, trash or folder.
ui-files-place-mount = Mount
ui-files-place-open = Open
ui-files-place-open-new-tab = Open in new tab
ui-files-place-rename = Rename…
ui-files-place-eject = Eject
ui-files-place-unmount = Unmount
ui-files-place-empty-trash = Empty Trash
ui-files-place-copy-path = Copy path
ui-files-place-remove-bookmark = Remove from bookmarks
ui-files-place-add-bookmark = Add to bookmarks
ui-files-place-properties = Properties

# Why a command is grayed out. The reason shows in the status line.
ui-files-action-nothing-made-in-trash = Nothing can be made in the Trash.
ui-files-action-nothing-to-paste = There is nothing to paste.
ui-files-action-paste-in-trash-folder = Nothing can be put inside a folder in the Trash.
ui-files-action-no-folder = Open a folder first.
ui-files-action-select-one-to-open = Select one item to open it.
ui-files-action-select-one-file-to-choose = Select one file to choose how to open it.
ui-files-action-only-folders-new-tab = Only folders open in a new tab.
ui-files-action-no-archive-tool = No archive tool is installed.
ui-files-action-archive-tool-missing = That archive tool is not installed.
ui-files-action-nothing-to-select = There is nothing to select.
ui-files-action-trash-empty = The Trash is empty.
ui-files-action-nothing-to-undo = There is nothing to undo.

# Names of undoable changes, as Edit > Undo shows them.
ui-files-undo-name-change = Change
ui-files-undo-name-rename = Rename
ui-files-undo-name-trash = Move to Trash
ui-files-undo-name-new-folder = New folder
ui-files-undo-name-new-file = New file
ui-files-undo-name-move = Move
ui-files-undo-name-duplicate = Duplicate
# Status after an undo or redo.
ui-files-undo-undone = Undone.
ui-files-undo-redone = Redone.
# $what (String) - the action that failed, such as "copy"
ui-files-transfer-what-undo = undo that
ui-files-transfer-what-redo = redo that
ui-files-transfer-what-move = move
ui-files-transfer-what-copy = copy
ui-files-transfer-what-rename = rename these items
ui-files-transfer-what-duplicate = duplicate
# $what (String) - what failed, $error (String) - why
ui-files-transfer-error = Could not { $what }: { $error }
ui-files-transfer-wait-busy = Wait for the current operation to finish.
ui-files-transfer-inside-itself = A folder cannot go inside itself.
ui-files-transfer-verb-moving = Moving
ui-files-transfer-verb-copying = Copying
ui-files-transfer-verb-duplicating = Duplicating
ui-files-transfer-past-moved = Moved
ui-files-transfer-past-copied = Copied
ui-files-transfer-renaming = Renaming…
ui-files-transfer-duplicated = Duplicated { $whole }.
ui-files-transfer-duplicating-stopped = Duplicating stopped.
# $verb (String) - the action in progress, $whole (String) - what it acts on
ui-files-transfer-busy-whole = { $verb } { $whole }
# $verb (String), $done (Number) - the item now on, $total (Number) - how many
ui-files-transfer-busy-count = { $verb } { $done } of { $total }
# $drive (String) - the drive being written
ui-files-transfer-finishing = Finishing… writing to { $drive }
# $past (String) - the finished action, $whole (String) - what it acted on
ui-files-transfer-done = { $past } { $whole }.
ui-files-transfer-done-flushed = { $past } { $whole }. Everything is written to { $drive }.
# $verb (String) - the stopped action
ui-files-transfer-stopped = { $verb } stopped.
# $count (Number) - how many items were renamed
ui-files-transfer-renamed = Renamed { $count ->
    [one] 1 item
   *[other] { $count } items
}.
# The conflict question. $name (String) - the clashing name, $place (String) - the folder it clashes in, already quoted.
ui-files-transfer-conflict-title = “{ $name }” already exists
ui-files-transfer-conflict-place = Something with this name is already in { $place }.
ui-files-transfer-conflict-the-destination = the destination
ui-files-transfer-apply-all = Do this for all conflicts
ui-files-transfer-skip = Skip
ui-files-transfer-keep-both = Keep both
ui-files-transfer-replace = Replace

# The status line after an action. $app (String) is the app's name, $error (String) why it failed.
ui-files-action-could-not-start = Could not start { $app }: { $error }.
# $count (Number) - how many paths were copied
ui-files-action-copied-paths = { $count ->
    [one] Path copied.
   *[other] Paths copied.
}
# $count (Number) - how many locations were copied
ui-files-action-copied-locations = { $count ->
    [one] Location copied.
   *[other] Locations copied.
}
ui-files-action-clipboard-unreachable = Could not reach the clipboard.
ui-files-action-bookmarks-failed = Could not change the bookmarks: { $error }.
ui-files-action-untitled-folder = Untitled folder
ui-files-action-untitled-file = Untitled file
ui-files-action-create-failed = Could not create it: { $error }
# $name (String) - the name of one item, shown in curly quotes
ui-files-action-quoted-name = “{ $name }”
# $count (Number) - how many items were picked
ui-files-action-item-count = { $count ->
    [one] { $count } item
   *[other] { $count } items
}

# The Trash prompts and toasts. $target (String) - what is meant, e.g. “report.txt” or 3 items.
ui-files-bin-moved = Moved { $target } to Trash
ui-files-bin-restored = Restored { $target }
ui-files-bin-delete-title = Delete { $target } permanently?
# $count (Number) - how many items are being deleted
ui-files-bin-delete-in-trash = { $count ->
    [one] It will be deleted permanently. This cannot be undone: nothing can bring it back.
   *[other] They will be deleted permanently. This cannot be undone: nothing can bring them back.
}
# $count (Number) - how many items are being deleted
ui-files-bin-delete-for-good = { $count ->
    [one] It will be deleted for good. This cannot be undone: it will not go to the Trash, and Undo cannot bring it back.
   *[other] They will be deleted for good. This cannot be undone: they will not go to the Trash, and Undo cannot bring them back.
}
ui-files-bin-empty-title = Empty the Trash?
# $count (Number) - how many items are in the Trash; $size (String) - its size in brackets, or empty
ui-files-bin-empty-warning = { $count ->
    [one] { $count } item{ $size } in the Trash will be deleted for good. This cannot be undone. It will not be recoverable.
   *[other] { $count } items{ $size } in the Trash will be deleted for good. This cannot be undone. They will not be recoverable.
}
# $count (Number) - how many items the Trash refused
ui-files-bin-refusal-several = { $count ->
    [one] It cannot be moved to the Trash.
   *[other] They cannot be moved to the Trash.
}
# $target (String) - what was refused; $reason (String) - the sentence saying why
ui-files-bin-refused-with-reason = { $target }: { $reason }
ui-files-bin-refused-all = { $target } cannot be moved to the Trash.
# One reason the Trash refused an item. $count (Number) - 1 for one item, 2 for several; $name (String) - a user folder's name.
ui-files-bin-refusal-not-absolute = The path must be absolute.
ui-files-bin-refusal-root = The root folder cannot be moved to the Trash.
ui-files-bin-refusal-trash-itself = The Trash itself cannot be moved to the Trash.
ui-files-bin-refusal-in-trash = { $count ->
    [one] It is in the Trash already.
   *[other] They are in the Trash already.
}
ui-files-bin-refusal-home = Your home folder cannot be moved to the Trash.
ui-files-bin-refusal-holds-home = { $count ->
    [one] It holds your home folder, so it cannot be moved to the Trash.
   *[other] They hold your home folder, so they cannot be moved to the Trash.
}
ui-files-bin-refusal-holds-trash = { $count ->
    [one] It holds the Trash, so it cannot be moved to the Trash.
   *[other] They hold the Trash, so they cannot be moved to the Trash.
}
ui-files-bin-refusal-user-folder = { $count ->
    [one] It is your { $name } folder, so it cannot be moved to the Trash.
   *[other] They are your { $name } folder, so they cannot be moved to the Trash.
}
ui-files-bin-refusal-read-only = { $count ->
    [one] It is on a read-only drive, so it cannot be moved to the Trash.
   *[other] They are on a read-only drive, so they cannot be moved to the Trash.
}
ui-files-bin-refusal-no-bin = { $count ->
    [one] Its drive has no Trash and one cannot be made there, so it cannot be moved to the Trash.
   *[other] Their drive has no Trash and one cannot be made there, so they cannot be moved to the Trash.
}
# $count (Number) - how many items are being copied to the Trash; picks “it” or “them”
ui-files-bin-copy-title = Copy { $target } to the Trash?
# $count (Number) - how many items are being copied to the Trash
ui-files-bin-copy-body = { $count ->
    [one] Moving it to the Trash means copying it to another drive first, which can take a long time and uses space there. To remove it without that, cancel and use Delete permanently from the menu opened with Shift.
   *[other] Moving them to the Trash means copying them to another drive first, which can take a long time and uses space there. To remove them without that, cancel and use Delete permanently from the menu opened with Shift.
}
ui-files-bin-copy-action = Copy to Trash
ui-files-bin-copying-busy = Copying { $target } to Trash
# $error (String) - why the move failed
ui-files-bin-move-failed = Could not move to the Trash: { $error }
ui-files-bin-deleting-busy = Deleting { $target }
ui-files-bin-deleted = Deleted { $target }.
ui-files-bin-delete-stopped = Deleting stopped.
# $error (String) - why the delete failed
ui-files-bin-delete-failed = Could not delete: { $error }
ui-files-bin-counting-busy = Counting what is in the Trash
ui-files-bin-already-empty = The Trash is already empty.
ui-files-bin-count-stopped = Counting stopped.
# $error (String) - why the count failed
ui-files-bin-count-failed = Could not count the Trash: { $error }
ui-files-bin-emptying-busy = Emptying the Trash
ui-files-bin-emptied = Emptied the Trash.
ui-files-bin-empty-stopped = Emptying stopped.
ui-files-bin-restore-to-title = Restore to
ui-files-bin-no-window = There is no window to choose a folder in.
# $count (Number) - how many items have no record; $target (String) - what is meant
ui-files-bin-no-record = { $count ->
    [one] { $target } has no record of where it came from. Use Restore to… to choose a folder.
   *[other] { $target } have no record of where they came from. Use Restore to… to choose a folder.
}
# $target (String) - what is meant, e.g. “report.txt” or 3 items.
ui-files-bin-make-folder-title = Make the folder again to restore { $target }?
# $count (Number) - how many folders are gone; $folder (String) - the first missing folder's path
ui-files-bin-make-folder-body = { $count ->
    [one] The folder { $folder } is no longer there. Restoring will make it again.
   *[other] { $count } folders, such as { $folder }, are no longer there. Restoring will make them again.
}
ui-files-bin-restoring-busy = Restoring { $target }
ui-files-bin-restore-stopped = Restoring stopped.
ui-files-bin-nothing-restored = Nothing was restored.
ui-files-bin-refuse-inside = Nothing can be put inside a folder in the Trash. Drop it on the Trash itself to move it there.
# The Extract dialog: the archive, where its files go, and what happens to a clash.
# $name (String) - the archive or folder name, shown in curly quotes
ui-extract-quoted-name = “{ $name }”
# $name (String) - the archive name
ui-extract-title = Extract “{ $name }”
ui-extract-needs-seven-zip = Opening a locked archive needs 7-Zip, which is not installed.
ui-extract-enter-password = Enter the password.
ui-extract-reading-archive = Reading the archive…
ui-extract-wrong-password = That password is not right.
ui-extract-to = Extract to
ui-extract-choose = Choose
ui-extract-names-hidden = Locked: the names are hidden too
# $count (Number) - how many items the archive holds
ui-extract-items-locked = { $count ->
    [one] { $count } item, locked
   *[other] { $count } items, locked
}
# $count (Number) - how many items the archive holds
ui-extract-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
ui-extract-reading = Reading…
ui-extract-group-archive = Archive
ui-extract-row-name = Name
ui-extract-row-contents = Contents
ui-extract-new-folder = Put the files in a new folder
ui-extract-folder-after-archive = A folder named after the archive.
ui-extract-files-loose = The files land loose in the folder.
ui-extract-if-taken = If a name is taken
ui-extract-then = Then
ui-extract-keep-both = Keep both
ui-extract-replace = Replace (the old one goes to the Trash)
ui-extract-skip = Skip it
ui-extract-password = Password
ui-extract-password-label = Password
ui-extract-action = Extract
ui-extract-checking-password = Checking the password…

# Shown under a name field when the name cannot be used.
ui-files-name-empty = Enter a name.
ui-files-name-reserved = A name cannot be . or ..
ui-files-name-slash = A name cannot contain a slash.
ui-files-name-too-long = The name is longer than 255 bytes.
ui-files-name-invalid = The name contains a character that is not allowed.
ui-files-name-exists = A file or folder with this name already exists.

# Batch rename dialog: title, form fields and preview rows.
# $count (Number) - how many items are being renamed
ui-batch-rename-title = { $count ->
    [one] Rename { $count } item
   *[other] Rename { $count } items
}
ui-batch-rename-action = Rename
ui-batch-rename-find-replace = Find and replace
ui-batch-rename-find = Find
ui-batch-rename-replace-with = Replace with
ui-batch-rename-match-case = Match case
ui-batch-rename-case = Case
ui-batch-rename-change-to = Change to
ui-batch-rename-keep-case = Keep as it is
ui-batch-rename-lowercase = lowercase
ui-batch-rename-uppercase = UPPERCASE
ui-batch-rename-title-case = Title Case
ui-batch-rename-number = Number
ui-batch-rename-add-number = Add a number
ui-batch-rename-no-number = No number
ui-batch-rename-after-name = After the name
ui-batch-rename-before-name = Before the name
ui-batch-rename-start-at = Start at
ui-batch-rename-new-names = New names
# $count (Number) - how many preview rows are hidden
ui-batch-rename-more = { $count ->
    [one] { $count } more item is not shown.
   *[other] { $count } more items are not shown.
}
# Row subtitles explaining why a new name cannot be used.
ui-batch-rename-problem-empty = A name cannot be empty.
ui-batch-rename-problem-slash = A name cannot contain “/”.
ui-batch-rename-problem-dots = A name cannot be “.” or “..”.
ui-batch-rename-problem-twice = Two items would have this name.
ui-batch-rename-problem-taken = Another item in the folder has this name.
ui-batch-rename-nothing-changes = Nothing would change.

# Compressing, reducing and extracting: the busy line, its toasts and the
# one-click archive names. $whole is a quoted name ("Photos") or a count.
ui-run-compress-busy = Compressing { $whole }
ui-run-reduce-busy = Reducing { $whole }
ui-run-extract-busy = Extracting { $whole }
ui-run-compress-counting = Compressing { $whole }: counting
ui-run-reduce-counting = Reducing { $whole }: counting
ui-run-extract-counting = Extracting { $whole }: counting
ui-run-compress-items = Compressing { $whole }, { $done } of { $total }
ui-run-reduce-items = Reducing { $whole }, { $done } of { $total }
ui-run-extract-items = Extracting { $whole }, { $done } of { $total }
# $index (Number) - the file being worked on; $total (Number) - how many files
ui-run-compress-file = Compressing { $index } of { $total }: { $name }
ui-run-reduce-file = Reducing { $index } of { $total }: { $name }
ui-run-extract-file = Extracting { $index } of { $total }: { $name }
# $percent (Number) - how far the file is, in whole per cent
ui-run-compress-file-percent = Compressing { $index } of { $total }: { $name }, { $percent }%
ui-run-reduce-file-percent = Reducing { $index } of { $total }: { $name }, { $percent }%
ui-run-extract-file-percent = Extracting { $index } of { $total }: { $name }, { $percent }%
# $drive (String) - the drive the archive is being written to
ui-run-finishing = Finishing, writing to { $drive }

ui-run-busy-wait = Wait for the current operation to finish.
ui-run-undone = Undone.
# $error (String) - the program's own message for the failed undo
ui-run-undo-failed = Could not undo that: { $error }
ui-run-show = Show
ui-run-undo = Undo
ui-run-compress-stopped = Compression stopped.
ui-run-reduce-stopped = Reducing stopped.
ui-run-extract-stopped = Extracting stopped.
# $error (String) - the program's own message
ui-run-compress-failed = Could not compress: { $error }
ui-run-reduce-failed = Could not reduce: { $error }
ui-run-extract-failed = Could not extract: { $error }

# $name (String) - the archive's file name, such as "Photos.zip"
ui-run-created = Created “{ $name }”
# $text (String) - the sentence before it; $count (Number) - the volumes made
ui-run-volumes = { $text } in { $count ->
    [one] 1 volume
   *[other] { $count } volumes
}
# $text (String) - the sentence before it; $problem (String) - the Trash error
ui-run-trash-failed = { $text }. The originals were not moved to the Trash: { $problem }
ui-run-trashed = { $text }. The originals are in the Trash
# $text (String) and $detail (String) - two sentences joined with a full stop
ui-run-with-detail = { $text }. { $detail }

# $name (String) - the extracted item's name
ui-run-extracted-to = Extracted to “{ $name }”
# $count (Number) - how many items were extracted
ui-run-extracted-items = { $count ->
    [one] Extracted 1 item
   *[other] Extracted { $count } items
}
# $count (Number) - how many were extracted; $skipped (Number) - how many were skipped
ui-run-extracted-items-skipped = { $count ->
    [one] Extracted 1 item, { $skipped } skipped because the name was taken
   *[other] Extracted { $count } items, { $skipped } skipped because the name was taken
}
ui-run-extract-nothing-taken = Nothing was extracted: every name was taken

# $name (String) - the one item's name, already quoted in the toast
ui-run-whole-name = “{ $name }”
# $count (Number) - how many items were selected
ui-run-whole-items = { $count ->
    [one] 1 item
   *[other] { $count } items
}

# How a finished reduction reads in a toast. $from and $to are sizes ("4.2 MB").
# $count (Number) - how many files were reduced. $only is 1 when nothing else happened to the others (none kept, none failed).
# $from (String), $to (String) - the total size before and after, such as "4.2 MB"
ui-reduce-done = { $only ->
    [1] { $count ->
        [one] Reduced from { $from } to { $to }
       *[other] Reduced { $count } files from { $from } to { $to }
    }
   *[other] { $count ->
        [one] Reduced 1 file from { $from } to { $to }
       *[other] Reduced { $count } files from { $from } to { $to }
    }
}
# $count (Number) - how many files were already small. $alone is 1 when no file was reduced.
ui-reduce-kept = { $alone ->
    [1] { $count ->
        [one] Already small: kept original
       *[other] { $count } already small: kept
    }
   *[other] { $count } already small: kept
}
ui-reduce-failed = { $count ->
    [one] 1 could not be reduced
   *[other] { $count } could not be reduced
}
ui-reduce-nothing = Nothing was reduced

# The Compress dialog. $count is the number of items; $size, $kbps, $crf and $kind are shown as given.
# $count (Number) - how many originals will be moved to the Trash after packing
ui-compress-confirm-pack-title = Move the originals to the Trash?
ui-compress-confirm-pack-body = { $count ->
    [one] Once the archive is made and checked, the original will be moved to the Trash. You can bring them back from there, or with Undo.
   *[other] Once the archive is made and checked, the { $count } originals will be moved to the Trash. You can bring them back from there, or with Undo.
}
ui-compress-confirm-pack-go = Move to Trash
# $count (Number) - how many originals will be replaced by smaller copies
ui-compress-confirm-reduce-title = Replace the originals?
ui-compress-confirm-reduce-body = { $count ->
    [one] Each smaller copy takes its original's place. Once it is made and checked, the original will be moved to the Trash. You can bring them back from there, or with Undo.
   *[other] Each smaller copy takes its original's place. Once they are made and checked, the { $count } originals will be moved to the Trash. You can bring them back from there, or with Undo.
}
ui-compress-confirm-reduce-go = Replace
ui-compress-save-in = Save in
ui-compress-choose = Choose
ui-compress-type = Type
ui-compress-archive = Archive
ui-compress-reduce-size = Reduce size
ui-compress-tools-busy = Looking for the tools that shrink media…
ui-compress-create = Create
ui-compress-name = Name
ui-compress-name-placeholder = Archive
ui-compress-conflict = Already exists
# $taken (String) - the name already in use here; $shown (String) - the name the new file will get
ui-compress-conflict-keep-body = “{ $taken }” is taken here, so this one will be “{ $shown }”.
# $shown (String) - the name of the file that will be replaced
ui-compress-conflict-replace-body = “{ $shown }” will be replaced. The old file goes to the Trash.
ui-compress-conflict-keep = Keep both
ui-compress-conflict-replace = Replace
ui-compress-format = Format
ui-compress-song-format = Song format
ui-compress-film-format = Film format
ui-compress-compression = Compression
ui-compress-level = Level
ui-compress-password = Password
ui-compress-lock = Ask for a password
ui-compress-lock-zip-only = Only .zip and .7z can be locked.
ui-compress-lock-aes = Encrypted with AES-256.
ui-compress-password-choose = Choose a password
ui-compress-password-field = Password
ui-compress-confirm-field = Confirm
ui-compress-hide-names = Encrypt file names
ui-compress-hide-names-hint = Nobody can even list what is inside without the password
ui-compress-split-one = One file
# $size (String) - the size of each volume, such as "700 MB"
ui-compress-split-each = { $size } each
ui-compress-volumes = Volumes
ui-compress-split = Split into volumes
ui-compress-split-hint = Cut a big archive into pieces, as for a disc or an email.
ui-compress-options = Options
ui-compress-hidden = Include hidden files
ui-compress-links = Keep links as links
ui-compress-links-hint = Off stores what each link points to instead
ui-compress-owners = Keep permissions and owners
ui-compress-owners-hint = Who may read each file, and who owns it.
ui-compress-owners-tar-only = Only .tar formats record who owns a file.
ui-compress-excludes = Leave out
ui-compress-excludes-placeholder = *.tmp, node_modules
ui-compress-excludes-hint = Comma separated. A name matches at any depth; a / makes it a path.
ui-compress-after = When it is done
ui-compress-trash-originals = Move the originals to the Trash
ui-compress-trash-originals-hint = Only after the archive is made and checked. You will be asked.
ui-compress-fastest = Fastest
ui-compress-balanced = Balanced
ui-compress-smallest = Smallest
ui-compress-custom = Custom
ui-compress-quality = Quality
ui-compress-crf = Constant rate factor
# $crf (Number) - the constant rate factor, a lower number is better quality
ui-compress-crf-value = CRF { $crf }
ui-compress-crf-hint = Higher quality on the right: a bigger file
ui-compress-resolution = Resolution
ui-compress-frame-rate = Frame rate
ui-compress-remove-sound = Remove sound
# $kbps (Number) - audio bitrate in kbit/s
ui-compress-kbps = { $kbps } kbit/s
ui-compress-sound = Sound
ui-compress-films = Films
ui-compress-remove-tags = Remove location and other tags
ui-compress-remove-tags-film-hint = Takes out the title, the place and the other tags
ui-compress-bitrate = Bitrate
ui-compress-flac-hint = FLAC keeps every sample; only the packing changes.
ui-compress-lossy-hint = Opus sounds fine at 96; MP3 wants 128 or more.
ui-compress-songs = Songs
ui-compress-remove-song-tags = Remove tags and cover
ui-compress-remove-song-tags-hint = Takes out the artist, the title and the picture
ui-compress-lossless = Lossless
ui-compress-png-hint = PNG keeps every pixel; only the packing changes.
ui-compress-lower-hint = Lower is smaller.
ui-compress-other = Other…
ui-compress-longest-side = Longest side
ui-compress-pixels = Pixels
ui-compress-pictures = Pictures
ui-compress-remove-location = Remove location and camera data
ui-compress-exif-hint = Takes out the EXIF, including where and with what it was taken. The colors stay.
ui-compress-checking = Checking
# $kind (String) - the kind of media, such as "Pictures"
ui-compress-media-cannot = { $kind } cannot be made smaller
# $size (String) - the total size now, such as "4.2 MB"; $error (String) - why no estimate could be made
ui-compress-no-estimate = Original { $size }, no estimate: { $error }
ui-compress-result = Result
ui-compress-size = Size
# $count (Number) - how many more files are not listed
ui-compress-and-more = and { $count } more
ui-compress-kept = kept as it is
# $size (String) - the estimated size, such as "2.1 MB"
ui-compress-about = about { $size }
ui-compress-replace-originals = Replace the originals
ui-compress-originals-trash-hint = The originals go to the Trash once the smaller files are made and checked. You will be asked.
ui-compress-replace-needs-beside = Replacing needs the result to sit beside the original.


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

## Files: drives, mounts and ejecting.
# Toast after a drive is safe to remove. $label is the drive's name.
ui-files-drive-safe-to-remove = { $label } is safe to remove.
# Toast when a drive is busy. $label is the drive's name.
ui-files-drive-busy = { $label } is busy. Close the files on it and try again.

## Files: the sidebar's own headings and rows

ui-files-places-bookmarks = Bookmarks
ui-files-places-drives = Drives
# $size (String) - the size of a drive that is not mounted
ui-files-places-not-mounted = Not mounted · { $size }
# $free (String) - the free space on a drive, e.g. "230 GB"
# $total (String) - the whole size of that drive, e.g. "465 GB"
ui-files-usage-free = { $free } free of { $total }
ui-files-place-desktop = Desktop
ui-files-place-documents = Documents
ui-files-place-downloads = Downloads
ui-files-place-music = Music
ui-files-place-pictures = Pictures
ui-files-place-videos = Videos

## Documents (open, save, close, recover)

# The name of a document that has no file yet.
ui-document-untitled = Untitled
# The second and later new documents. $name - the name above, $number (Number) - 2, 3, ...
ui-document-untitled-numbered = { $name } { $number }
ui-document-chooser-open = Open
ui-document-chooser-save-as = Save as
ui-document-no-recent = No recent documents
# A toast. $name - the file's name
ui-document-reloaded = “{ $name }” was reloaded after another program changed it.
ui-document-cancel = Cancel
ui-document-close = Close
ui-document-discard = Discard
ui-document-discard-all = Discard all
ui-document-save = Save
ui-document-save-all = Save all
ui-document-save-as = Save as…
ui-document-try-again = Try again
ui-document-replace = Replace
ui-document-revert = Revert
ui-document-not-now = Not now
ui-document-recover = Recover
ui-document-keep-my-version = Keep my version
ui-document-reload = Reload
ui-document-keep-open = Keep open
# The end of a list of names. $count (Number) - how many are not listed
ui-document-more = { $count } more
# $name - the document's name
ui-document-unsaved-title = Save changes to “{ $name }”?
# $count (Number) - how many documents have unsaved changes
ui-document-unsaved-title-many = Save changes to { $count ->
    [one] { $count } document
   *[other] { $count } documents
}?
ui-document-unsaved-body = If you do not save, your changes are lost.
# $names - a list of document names
ui-document-unsaved-body-many = { $names } have unsaved changes. If you do not save them, they are lost.
# $name - the document's name
ui-document-recover-title = Recover “{ $name }”?
# $app - the app's name, $when - how long ago
ui-document-recover-body = { $app } closed before you saved this document. Its last copy was kept { $when }.
# $count (Number) - how many documents
ui-document-recover-title-many = Recover { $count ->
    [one] { $count } unsaved document
   *[other] { $count } unsaved documents
}?
# $app - the app's name, $names - a list of names, $when - how long ago
ui-document-recover-body-many = { $app } closed before you saved { $names }. The latest copy was kept { $when }.
ui-document-changed-title = “{ $name }” changed on disk
ui-document-changed-body = Another program saved this file after you opened it. You also have unsaved changes here. Reloading replaces them.
ui-document-removed-title = “{ $name }” is no longer there
ui-document-removed-body = It was deleted or moved. Your copy stays open. Save it to keep it.
ui-document-conflict-title = Replace “{ $name }”?
ui-document-conflict-body = Another program changed this file after you opened it. Saving replaces its changes.
ui-document-save-failed-title = Could not save “{ $name }”
# $name - the file's name, $advice - what to do about it
ui-document-save-failed-message = Could not save “{ $name }”. { $advice }
ui-document-open-failed-title = Could not open “{ $name }”
ui-document-open-failed-message = Could not open “{ $name }”. { $advice }
ui-document-revert-title = Revert “{ $name }”?
ui-document-revert-body = The file is read again, and your unsaved changes are lost.
ui-document-open-elsewhere-title = “{ $name }” is already open
ui-document-open-elsewhere-body = Close that document first, or save under another name.
ui-document-save-error-permission = You do not have permission to change it. Save a copy somewhere else.
ui-document-save-error-full = The drive is full. Free some space, then save again.
ui-document-save-error-folder = Its folder is no longer there. Save a copy somewhere else.
ui-document-save-error-read-only = The drive is read-only. Save a copy somewhere else.
ui-document-save-error-name = The name is not allowed here. Choose another name.
ui-document-save-error-other = Check that the drive is connected, then try again.
ui-document-open-error-missing = It is no longer there. Check the name, or look for it in Files.
ui-document-open-error-permission = You do not have permission to read it.
ui-document-open-error-folder = It is a folder, not a file.
ui-document-open-error-too-large = It is too big to open here.
ui-document-open-error-other = Check that the drive is connected, then try again.

## Media: the player's transport, seek bar, volume, now-playing bar and the
## spoken times a screen reader reads out.

ui-media-play-tooltip = Play (Space)
ui-media-pause-tooltip = Pause (Space)
ui-media-next = Next
ui-media-previous = Previous
ui-media-shuffle = Shuffle
ui-media-shuffle-on = Shuffle is on
ui-media-repeat = Repeat
ui-media-repeat-all = Repeating all
ui-media-repeat-one = Repeating one
ui-media-skip-back = Back 10 seconds
ui-media-skip-forward = Forward 10 seconds
ui-media-volume = Volume
ui-media-mute-tooltip = Mute (M)
ui-media-unmute-tooltip = Unmute (M)
ui-media-seek = Seek
ui-media-video = Video
ui-media-show-time-left = Show time left
ui-media-show-length = Show length

# A screen reader's value for the seek bar: $current and $total are spoken times ("3 minutes 45 seconds").
# $current (String) - the position, spoken; $total (String) - the length, spoken
ui-media-spoken-of = { $current } of { $total }
# $first and $second (String) - two spoken durations, one after the other
ui-media-spoken-join = { $first } { $second }
# $count (Number) - a number of hours
ui-media-spoken-hours = { $count ->
    [one] 1 hour
   *[other] { $count } hours
}
# $count (Number) - a number of minutes
ui-media-spoken-minutes = { $count ->
    [one] 1 minute
   *[other] { $count } minutes
}
# $count (Number) - a number of seconds
ui-media-spoken-seconds = { $count ->
    [one] 1 second
   *[other] { $count } seconds
}

# A screen reader's name for a now-playing item. $title and $subtitle are the film or song's names.
# $title (String) - the title; $subtitle (String) - the artist or year
ui-media-item-a11y = { $title }, { $subtitle }. Open the player

# A screen reader's name for one page of a carousel.
# $page (Number) - the page's position from 1; $count (Number) - how many pages
ui-carousel-page-a11y = Page { $page } of { $count }

# A screen reader's name for one page dot.
# $name (String) - the page's name, such as a place; $page (Number) - its position from 1; $count (Number) - how many pages
ui-page-dot-a11y = { $name }, page { $page } of { $count }

## Files: the Properties dialog

# The title of the dialog. $name (String) - the item's name, or how many items
ui-files-props-title = Properties of { $name }
ui-files-props-header = Properties
ui-files-props-close = Close
ui-files-props-stop = Stop
ui-files-props-cancel = Cancel
ui-files-props-apply = Apply
ui-files-props-apply-ellipsis = Apply…
ui-files-props-none = None
ui-files-props-mixed = Mixed
# $message (String) - why the file could not be read
ui-files-props-read-failed = Could not read everything: { $message }
ui-files-props-several-folders = Several folders
ui-files-props-times = Times
ui-files-props-created = Created
ui-files-props-modified = Modified
ui-files-props-accessed = Accessed
ui-files-props-general = General
ui-files-props-type = Type
ui-files-props-mime-type = MIME type
ui-files-props-size = Size
ui-files-props-contents = Contents
ui-files-props-location = Location
ui-files-props-original-location = Original location
ui-files-props-deleted = Deleted
ui-files-props-link-target = Link target
ui-files-props-open-with = Open with
ui-files-props-applications = Applications
ui-files-props-none-found = None found
# $name (String) - the application, $error (String) - what went wrong
ui-files-props-open-failed = Could not start { $name }: { $error }.
ui-files-props-volume = Volume
ui-files-props-volume-name = Name
ui-files-props-device = Device
ui-files-props-mounted-at = Mounted at
ui-files-props-permissions = Permissions
ui-files-props-permissions-several = Who may read, write and run each of these.
ui-files-props-permissions-single = Who may read, write and run this.
ui-files-props-who-can-change = Who can change
ui-files-props-access-nobody-read-only = Nobody, read-only drive
ui-files-props-access-you = You
ui-files-props-access-owners = Only the owners
ui-files-props-access-owner = Only the owner
ui-files-props-who-owner = Owner
ui-files-props-who-group = Group
ui-files-props-who-others = Others
ui-files-props-can-read = Read
ui-files-props-can-write = Write
# The column of the permission grid for running a file.
ui-files-props-can-execute = Execute
# The same column for a folder, where it means going into it.
ui-files-props-can-access = Access
ui-files-props-mode = Mode
ui-files-props-in-letters = In letters
ui-files-props-special = Special
ui-files-props-set-user-id = Set user ID
ui-files-props-set-group-id = Set group ID
ui-files-props-sticky = Sticky
# $name (String) - one of the special bits, set on some of the items
ui-files-props-special-some = { $name } (some)
ui-files-props-allow-executing = Allow executing as program
ui-files-props-ownership = Ownership
ui-files-props-owner = Owner
ui-files-props-group = Group
ui-files-props-several-owners = Several owners
ui-files-props-several-groups = Several groups
ui-files-props-enclosed-items = Enclosed items
ui-files-props-enclosed-hint = Files keep an execute bit only where they had one
ui-files-props-apply-enclosed = Apply to enclosed items
ui-files-props-apply-everything-inside = Apply to everything inside?
# $mode (String) - the mode in octal, such as 0644
ui-files-props-apply-mode-inside = Apply { $mode } to all items inside?
ui-files-props-applying = Applying…
ui-files-props-apply-stopped = Stopped. What was changed stays changed.
ui-files-props-security = Security
ui-files-props-selinux-context = SELinux context
ui-files-props-acl = Access control list
ui-files-props-acl-entries = Has entries beyond owner, group and others
ui-files-props-total-size = Total size
ui-files-props-items-label = Items
ui-files-props-files-label = Files
ui-files-props-folders-label = Folders
ui-files-props-largest-items = Largest items
ui-files-props-empty-folder = This folder is empty
ui-files-props-image = Image
ui-files-props-dimensions = Dimensions
ui-files-props-camera = Camera
ui-files-props-lens = Lens
ui-files-props-taken = Taken
ui-files-props-exposure = Exposure
ui-files-props-audio = Audio
ui-files-props-video = Video
ui-files-props-duration = Duration
ui-files-props-codec = Codec
ui-files-props-copy = Copy
ui-files-props-copied = Copied.
ui-files-props-copy-failed = The text could not be copied.
ui-files-props-calculate = Calculate
ui-files-props-hash-done = Done
ui-files-props-hash-stopped = Stopped
ui-files-props-checksum = Checksum
# The name of the hash algorithm, which is the same in every language.
ui-files-props-sha-256 = SHA-256
ui-files-props-storage = Storage
ui-files-props-inode = Inode
ui-files-props-hard-links = Hard links
ui-files-props-block-size = Block size
ui-files-props-disk-usage = Disk usage
ui-files-props-extended-attributes = Extended attributes
ui-files-props-attributes = Attributes

## Text editor: commands and undo steps

# The heading of the line and comment commands in menus and the palette.
ui-text-group = Text
ui-text-undo = Undo
# $edit (String) - what the step did, such as "typing"
ui-text-undo-edit = Undo { $edit }
ui-text-redo = Redo
# $edit (String) - what the step did, such as "typing"
ui-text-redo-edit = Redo { $edit }
ui-text-edit-typing = typing
ui-text-edit-delete = delete
ui-text-edit-delete-word = delete word
ui-text-edit-indent = indent
ui-text-edit-outdent = outdent
ui-text-edit-cut = cut
ui-text-edit-paste = paste
ui-text-edit-replace = replace
ui-text-edit-drop-text = drop text
ui-text-edit-move-text = move text
ui-text-edit-duplicate-line = duplicate line
ui-text-edit-delete-line = delete line
ui-text-edit-move-line = move line
ui-text-edit-join-lines = join lines
ui-text-edit-comment = comment
ui-text-edit-sort-lines = sort lines
ui-text-edit-remove-trailing-spaces = remove trailing spaces
ui-text-edit-uppercase = uppercase
ui-text-edit-lowercase = lowercase
ui-text-edit-title-case = title case
ui-text-cmd-cut = Cut
ui-text-cmd-copy = Copy
ui-text-cmd-paste = Paste
ui-text-cmd-select-all = Select all
ui-text-cmd-select-line = Select line
ui-text-cmd-add-next-occurrence = Add next occurrence
    .keywords = multiple carets, cursors
ui-text-cmd-select-all-occurrences = Select every occurrence
    .keywords = multiple carets, cursors
ui-text-cmd-caret-above = Add caret above
    .keywords = cursor, column
ui-text-cmd-caret-below = Add caret below
    .keywords = cursor, column
ui-text-cmd-caret-click = Add or remove a caret
ui-text-cmd-matching-bracket = Go to matching bracket
ui-text-cmd-comment = Comment or uncomment
    .keywords = toggle comment, uncomment
ui-text-cmd-duplicate-line = Duplicate line
    .keywords = copy line
ui-text-cmd-delete-line = Delete line
ui-text-cmd-move-line-up = Move line up
ui-text-cmd-move-line-down = Move line down
ui-text-cmd-join-lines = Join lines
ui-text-cmd-indent = Indent or outdent
ui-text-cmd-delete-word = Delete a word
ui-text-cmd-sort-lines = Sort lines
ui-text-cmd-trim-trailing = Remove trailing spaces
    .keywords = whitespace, trim
ui-text-cmd-uppercase = Uppercase
    .keywords = capitals, case
ui-text-cmd-lowercase = Lowercase
    .keywords = case
ui-text-cmd-title-case = Title case
    .keywords = capitalise, case

## Spoken sentences for lists and tables

# $count (Number) - how many rows are selected; said when the selection changes
ui-a11y-selected = { $count ->
    [0] Nothing selected
    [one] 1 selected
   *[other] { $count } selected
}
# $column (String) - the column's title. $direction is "ascending" or "descending".
ui-a11y-sorted = Sorted by { $column }, { $direction ->
    [ascending] ascending
   *[descending] descending
}

## Calendar widgets: the month header, the "more" chip and the time picker

ui-calendar-previous-month = Previous month
ui-calendar-next-month = Next month
# $count (Number) - how many more events a day has than fit in its cell
ui-calendar-more = +{ $count } more
# $count (Number) - how many more events there are. $date (String) - the day, written out
ui-calendar-more-a11y = { $count ->
    [one] 1 more event on { $date }
   *[other] { $count } more events on { $date }
}
# $minutes (Number) - a length under an hour
ui-calendar-length-minutes = { $minutes } min
# $hours (Number) - a length of an hour or more, 1.5 for an hour and a half
ui-calendar-length-hours = { NUMBER($hours, maximumFractionDigits: 1) } h
# A time of day with how long after the start it is. $time (String) - "10:30"; $length (String) - "30 min"
ui-calendar-time-option = { $time }, { $length }
ui-calendar-time = Time

## The find card: which match of how many, and its buttons

ui-findcard-none-short = None
ui-findcard-none = Phrase not found
# $current (Number) - the match shown. $total (Number) - how many matches
ui-findcard-position-short = { $current }/{ $total }
ui-findcard-position = { $current } of { $total }
ui-findcard-previous =
    .tooltip = Previous match
ui-findcard-next =
    .tooltip = Next match
ui-findcard-close =
    .tooltip = Close (Escape)

## Other controls

# The accessible name of the menu half of a split button when the app gives none.
ui-split-menu-label = More options
ui-omnibox-label = Location and search
ui-property-reset =
    .tooltip = Reset to the app’s value

## The tab strip's own buttons

ui-tab-muted =
    .a11y = Muted
ui-tab-playing =
    .a11y = Playing sound
# $title (String) - the tab's title
ui-tab-mute =
    .tooltip = Mute { $title }
ui-tab-unmute =
    .tooltip = Unmute { $title }
ui-tab-close-named =
    .tooltip = Close { $title }
ui-tab-list =
    .tooltip = Tab list
ui-tab-new =
    .tooltip = New tab

## The crop box: what a screen reader says about each handle

ui-crop-grip-move = Crop area
ui-crop-grip-north = Top edge
ui-crop-grip-south = Bottom edge
ui-crop-grip-east = Right edge
ui-crop-grip-west = Left edge
ui-crop-grip-north-east = Top right corner
ui-crop-grip-north-west = Top left corner
ui-crop-grip-south-east = Bottom right corner
ui-crop-grip-south-west = Bottom left corner
ui-crop-turn =
    .tooltip = Turn the shape on its side
# $width (Number), $height (Number) - the size of the crop in pixels
ui-crop-size = { $width } × { $height }

## Moving to and deleting from the Trash

# $target (String) - the item's name in quotes, or a count of items
ui-files-bin-moving-busy = Moving { $target } to Trash
# $count (Number) - how many items were selected
ui-files-bin-permission-error = { $count ->
    [one] You do not have permission to move it to the Trash.
   *[other] You do not have permission to move them to the Trash.
}
ui-files-transfer-what-trash = move to the Trash
ui-files-transfer-verb-deleting = Deleting
ui-files-transfer-verb-emptying = Emptying
ui-files-transfer-verb-restoring = Restoring
ui-files-bin-whole-trash = the Trash

## What kind of file it is

ui-file-kind-folder = Folder
ui-file-kind-image = Image
ui-file-kind-audio = Audio
ui-file-kind-video = Video
ui-file-kind-text = Text
ui-file-kind-code = Code
ui-file-kind-config = Data
ui-file-kind-markup = Markup
ui-file-kind-pdf = PDF
ui-file-kind-archive = Archive
ui-file-kind-disk-image = Disk image
ui-file-kind-spreadsheet = Spreadsheet
ui-file-kind-presentation = Presentation
ui-file-kind-document = Document
ui-file-kind-page = Other document
ui-file-kind-ebook = E-book
ui-file-kind-font = Font
ui-file-kind-executable = Program
ui-file-kind-model-3d = 3D model
ui-file-kind-database = Database
ui-file-kind-certificate = Certificate
ui-file-kind-torrent = Torrent
ui-file-kind-unknown = Unknown

## Tables: the batch bar, column menu, sorting and empty states

ui-table-clear-selection =
    .tooltip = Clear selection
ui-table-batch =
    .a11y = Actions for the selected rows
# $count (Number) - how many rows are selected
ui-table-selected = { $count } selected
ui-table-reset-columns = Reset columns
ui-table-retry = Try again
ui-table-empty = Nothing to show
ui-table-sort-ascending = Ascending
ui-table-sort-descending = Descending
ui-table-sort = Sort
# $column (String) - the title of the column the table is sorted by
ui-table-sort-by = Sort by { $column }
ui-table-select-all =
    .a11y = Select all rows

## Tab commands: the File and Window menus and the shortcuts sheet

ui-tab-cmd-new = New tab
ui-tab-cmd-next = Next tab
ui-tab-cmd-previous = Previous tab
ui-tab-cmd-go-to = Go to a tab
    .keys = Alt+1 to 9

## Choice lists, menu type-ahead, navigation and lists

ui-rows-copy-path = Copy path
ui-rows-no-match-empty = Nothing matches.
# $count (Number) - how many choices are not shown
ui-rows-more-choices = { $count } more. Keep typing to narrow the list.
# What a screen reader says when the typed letters change which menu rows match.
# $count (Number) - how many rows match; $query (String) - the typed letters
ui-menu-query-announcement = { $count ->
    [0] No matches for { $query }
    [one] 1 match for { $query }
   *[other] { $count } matches for { $query }
}
# $count (Number) - how many rows match the typed letters
ui-menu-query-count = { $count ->
    [0] No matches
    [one] 1 match
   *[other] { $count } matches
}
ui-nav-not-found = Not found
ui-nav-keep-editing = Keep editing
ui-nav-page-not-found-title = Page not found
ui-nav-go-up = Go up
ui-nav-go-home = Go home
ui-list-empty = Nothing to show
ui-location-source-portal = From the location service
ui-location-source-network = Approximate, from the network address
ui-format-invalid-date = Invalid date
ui-format-invalid-time = Invalid time

## Previews: pictures, films and PDF search

# $name (String) - the picture's file name
ui-preview-picture-a11y = Picture: { $name }
ui-preview-play =
    .tooltip = Play (K)
ui-preview-pause =
    .tooltip = Pause (K)
ui-preview-play-again =
    .tooltip = Play again (K)
ui-preview-subtitles-off = Off
ui-preview-find-searching = Searching…
ui-preview-find-none = No matches
# $current (Number) - the match shown; $total (Number) - how many matches; $more is 1 while the search goes on
ui-preview-find-position = { $current } of { $total }{ $more ->
    [1] +
   *[other] {""}
}

## Zoom view and toolbar

ui-zoom-out =
    .tooltip = Zoom out (-)
ui-zoom-in =
    .tooltip = Zoom in (+)
ui-zoom-actual =
    .tooltip = Actual size (1)
ui-zoom-fit =
    .tooltip = Fit to window (0)
# The accessible name of a toolbar.
ui-toolbar-label = Toolbar
ui-toolbar-reset = Reset toolbar
ui-toolbar-customise = Customise toolbar
# The overflow button that holds the toolbar items that do not fit.
ui-toolbar-more = More

## Text editor: the find bar, Go to line and the status line

ui-text-find-searching = Searching…
ui-text-find-none = No results
ui-text-find-pattern-invalid = Not a valid pattern
ui-text-find-pattern-too-large = The pattern is too large
# $current (Number) - the match shown; $total (Number) - how many matches; $more is 1 when the count stopped at its limit
ui-text-find-position = { $current } of { $total }{ $more ->
    [1] +
   *[other] {""}
}
# $total (Number) - how many matches; $more is 1 when the count stopped at its limit
ui-text-find-count = { $more ->
    [1] { $total }+ results
   *[other] { $total ->
        [one] 1 result
       *[other] { $total } results
    }
}
ui-text-find-options = Search options
ui-text-find-match-case = Match case
    .tooltip = Match case (Alt+C)
ui-text-find-whole-words = Whole words
    .tooltip = Whole words (Alt+W)
ui-text-find-regex = Regular expression
    .tooltip = Regular expression (Alt+R)
    .placeholder = Regular expression
ui-text-find-query =
    .placeholder = Find
ui-text-find-replace-show =
    .tooltip = Show replace
ui-text-find-replace-hide =
    .tooltip = Hide replace
ui-text-find-previous =
    .tooltip = Previous result (Shift+Enter)
ui-text-find-next =
    .tooltip = Next result (Enter)
ui-text-find-close =
    .tooltip = Close (Escape)
ui-text-find-replacement =
    .placeholder = Replace with
ui-text-find-replace = Replace
ui-text-find-replace-all = Replace all
# $current (Number) - the line the caret is on; $lines (Number) - how many lines the document has
ui-text-goto-current = Line { $current } of { $lines }
# $line (Number), $column (Number) - where the typed text would go
ui-text-goto-line-column = Line { $line }, column { $column }
ui-text-goto-line = Line { $line }
ui-text-goto-invalid = Line number or line:column
ui-text-goto-field =
    .placeholder = Go to line
# $count (Number) - how many carets there are
ui-text-caret-selections = { $count ->
    [one] 1 selection
   *[other] { $count } selections
}
# $chars (Number) - how many characters are selected; $lines (Number) - how many lines they cover
ui-text-caret-selected = { $lines ->
    [one] { $chars ->
        [one] 1 character selected
       *[other] { $chars } characters selected
    }
   *[other] { $chars ->
        [one] 1 character, { $lines } lines selected
       *[other] { $chars } characters, { $lines } lines selected
    }
}
# $line (Number), $column (Number) - where the caret is, counting from 1
ui-text-caret-position = Line { $line }, Column { $column }

## Files: the location field's chips, filters and search status

ui-files-omni-scope-folder = This folder
ui-files-omni-scope-home = Home
ui-files-omni-scope-everywhere = Everywhere
# Completes "Search in …".
ui-files-omni-phrase-home = Home
ui-files-omni-phrase-everywhere = everywhere
ui-files-omni-kind-folders = Folders
ui-files-omni-kind-images = Images
ui-files-omni-kind-video = Video
ui-files-omni-kind-audio = Audio
ui-files-omni-kind-documents = Documents
ui-files-omni-kind-archives = Archives
ui-files-omni-kind-code = Code
ui-files-omni-when-today = Today
ui-files-omni-when-week = This week
ui-files-omni-when-year = This year
ui-files-omni-size-small = Under 1 MB
ui-files-omni-size-large = Over 100 MB
# The chip of an active filter. $kinds (String) - the types, joined with commas
ui-files-omni-token-type = Type: { $kinds }
# $extensions (String) - the extensions with their dots, joined with commas
ui-files-omni-token-extension = Extension: { $extensions }
# $size (String) - a size such as "10 MB"
ui-files-omni-token-size-over = Size over { $size }
ui-files-omni-token-size-at-least = Size at least { $size }
ui-files-omni-token-size-under = Size under { $size }
ui-files-omni-token-size-at-most = Size at most { $size }
ui-files-omni-token-size-exactly = Size { $size }
# $low (String), $high (String) - two sizes
ui-files-omni-token-size-between = Size { $low } to { $high }
ui-files-omni-token-modified-today = Modified today
ui-files-omni-token-modified-yesterday = Modified yesterday
# $days (Number) - how many days back
ui-files-omni-token-modified-days = { $days ->
    [one] Modified in the last day
   *[other] Modified in the last { $days } days
}
ui-files-omni-token-modified-year = Modified this year
# $moment (String) - a year, month or day as typed
ui-files-omni-token-modified-on = Modified in { $moment }
ui-files-omni-token-modified-since = Modified since { $moment }
ui-files-omni-token-modified-before = Modified before { $moment }
# $from (String), $to (String) - two days as typed
ui-files-omni-token-modified-between = Modified { $from } to { $to }
ui-files-omni-name-starts = starts with words
ui-files-omni-name-exact = is exactly the words
ui-files-omni-name-regex = regular expression
# $mode (String) - how the name is matched; $globs (String) - the patterns, joined with commas
ui-files-omni-token-name-mode-globs = Name { $mode }, { $globs }
ui-files-omni-token-name-mode = Name { $mode }
ui-files-omni-token-name-globs = Name { $globs }
# $folder (String) - the folder the search is limited to
ui-files-omni-token-within = In { $folder }
ui-files-omni-token-hidden-include = Including hidden files
ui-files-omni-token-hidden-only = Only hidden files
ui-files-omni-token-hidden-exclude = Without hidden files
ui-files-omni-token-case = Match case
ui-files-omni-status-searching = Searching…
# $found (Number) - how many results so far
ui-files-omni-status-found-searching = { $found } found, searching…
# $found (Number) - how many results. $limit is 1 when the search stopped at its limit.
# $unreadable (Number) - how many folders could not be read
ui-files-omni-status-found = { $found } found{ $limit ->
    [1] {" · limit reached, narrow the search"}
   *[other] { $unreadable ->
        [0] {""}
        [one] {" · 1 folder could not be read"}
       *[other] {" · "}{ $unreadable } folders could not be read
    }
}

## Files: the Compress dialog's rules and sentences

# $count (Number) - how many items; $name (String) - the one item's name when there is one
ui-compress-title = { $count ->
    [one] Compress “{ $name }”
   *[other] Compress { $count } items
}
ui-compress-dimension-empty-error = Enter a size in pixels.
ui-compress-dimension-digits-error = Use digits only, such as 1600.
# $least (String), $most (String) - the smallest and largest size, in pixels
ui-compress-dimension-range-error = Use a size from { $least } to { $most } pixels.
ui-compress-password-empty-error = Enter a password, or turn the password off.
ui-compress-password-mismatch-error = The passwords do not match.
ui-compress-tools-looking-status = Looking for the tools that shrink media…
ui-compress-nothing-smaller-error = There is nothing here that can be made smaller.
ui-compress-original-size = Original size
ui-compress-original-rate = Original rate
# $px (String) - a size in pixels, such as 1920
ui-compress-px = { $px } px
# $px (String) - a video height, such as 1080, written as 1080p
ui-compress-height-p = { $px }p
# $fps (String) - frames a second
ui-compress-fps = { $fps } fps
ui-compress-px-suffix = px
# $original (String), $estimated (String) - sizes such as "842 MB"
ui-compress-summary = Original { $original }, estimated { $estimated }
ui-compress-summary-estimating = Original { $original }, estimating…
# $count (Number) - how many files were selected
ui-run-whole-files = { $count ->
    [one] 1 file
   *[other] { $count } files
}
# $label (String) - the drive's name; $sentence (String) - why it could not be opened
ui-files-drive-error = { $label }: { $sentence }

## Files: why an operation did not happen. One message per kind of bingux_files::ops::OpError.

ui-files-error-exists = Something with that name already exists.
ui-files-error-permission = You do not have permission to do that.
ui-files-error-not-found = That file or folder no longer exists.
ui-files-error-cancelled = The operation was cancelled.
ui-files-error-name-empty = Enter a name for the file.
ui-files-error-name-reserved = A file cannot be called . or ..
ui-files-error-name-too-long = The name is longer than 255 bytes.
ui-files-error-name-invalid = The name contains a character that is not allowed.
# $program (String) - the name of a program that is not installed, such as zip
ui-files-error-missing = { $program } is not installed.
ui-files-error-copy-needed = Moving it to the Trash means copying it to another place first, which can take a long time.
ui-files-error-io-full = The drive is full. Free some space, then try again.
ui-files-error-io-read-only = The drive is read-only. Choose another place.
ui-files-error-io-quota-full = Your storage quota is full.
ui-files-error-io-not-empty = The folder is not empty.
ui-files-error-io-in-use = It is in use by another app. Close it there, then try again.
ui-files-error-io-between-drives = That cannot be done between two drives in one step.
ui-files-error-io-timed-out = The drive took too long to answer. Try again.
ui-files-error-io-not-a-folder = That is not a folder.
ui-files-error-io-is-a-folder = That is a folder, not a file.
ui-files-error-io-bad-name = Could not use that name. Choose another name.
ui-files-error-io-unsupported = This drive cannot do that.
ui-files-error-io-interrupted = It was interrupted. Try again.
ui-files-error-io-unreadable = Could not read or write the drive. Check that it is connected, then try again.

## Files: the problems of a compress name or an exclude pattern.

ui-compress-name-empty = Enter a name for the archive.
ui-compress-name-has-slash = A name cannot contain a slash.
ui-compress-name-reserved = That name cannot be used.
ui-compress-name-too-long = That name is too long.
ui-compress-name-taken = A file with this name already exists here.
# $pattern (String) - what was typed; the reason follows in its own message
ui-files-pattern-too-many = Too many patterns: the most is 64.
ui-files-pattern-too-long = { $pattern }: the pattern is too long.
ui-files-pattern-null = { $pattern }: a pattern cannot hold a null character.
ui-files-pattern-backslash = { $pattern }: a backslash must be followed by a character.
ui-files-pattern-open-class = { $pattern }: a [ is never closed.
# $count (Number) - megabytes
ui-compress-split-mb = { $count } MB
# $count (Number) - gigabytes
ui-compress-split-gb = { $count } GB

## Files: the step a program was on when it failed; the lead of ui-files-failure-tool-failed.

ui-files-step-make-archive = The archive could not be made
ui-files-step-check-archive = The new archive did not pass its check
ui-files-step-list-new-archive = The new archive could not be listed
ui-files-step-estimate-size = The size could not be estimated
ui-files-step-reduce = It could not be reduced
ui-files-step-read-archive = The archive could not be read
ui-files-step-try-password = The password could not be tried
ui-files-step-extract = The archive could not be extracted
ui-files-step-other = It failed

## Files: one message per kind of bingux_files::ops::Failure.

ui-files-failure-work-folder = A work folder could not be made.
ui-files-failure-nothing-to-compress = There is nothing to compress.
ui-files-failure-everything-left-out = There is nothing to compress: every item was left out.
ui-files-failure-nothing-to-reduce = There is nothing to reduce.
ui-files-failure-paths-not-absolute = The paths must be absolute.
ui-files-failure-no-common-parent = The paths have nothing in common.
ui-files-failure-root-not-compressible = The root folder cannot be compressed.
ui-files-failure-destination-not-folder = The destination is not a folder.
ui-files-failure-originals-hold-archive = The originals cannot be removed when the archive is saved inside them.
ui-files-failure-program-wrote-nothing = The archive could not be made: the program wrote nothing.
# $listed (Number) - entries in the new archive; $expected (Number) - entries put in
ui-files-failure-archive-missing-items = The new archive is missing items: it holds { $listed } of { $expected }.
# $name (String) - the folder's name, quoted; $why (String) - the reason, a sentence
ui-files-failure-folder-unreadable = { $name } could not be read: { $why }
# $name (String) - the file's name, quoted
ui-files-failure-file-no-permission = { $name } cannot be read: you do not have permission.
# $name (String) - the name, quoted; $extension (String) - the format's ending, such as zip
ui-files-failure-name-not-storable = { $name } has a name that { $extension } cannot store. Choose .zip or a .tar format.
# $name (String) - the name, quoted
ui-files-failure-name-not-storable-locked = { $name } has a name that a locked .zip cannot store. Remove the password or choose .tar.
# $label (String) - the drive's name; $why (String) - the reason, a sentence
ui-files-failure-drive-write-named = Could not write to { $label }: { $why }
# $why (String) - the reason, a sentence
ui-files-failure-drive-write = Could not write to the drive: { $why }
# $label (String) - the drive's name; $why (String) - the reason, a sentence
ui-files-failure-drive-finish = Could not finish writing to { $label }: { $why }
ui-files-failure-root-rename = The root folder cannot be renamed.
ui-files-failure-root-delete = The root folder cannot be deleted.
ui-files-failure-inside-itself = A folder cannot be put inside itself.
ui-files-failure-cannot-copy-or-move = That cannot be copied or moved.
ui-files-failure-cannot-trash = That cannot be moved to the Trash.
# $name (String) - the name, quoted
ui-files-failure-did-not-arrive = { $name } did not arrive complete, so the original was left where it is.
# $name (String) - the name, quoted
ui-files-failure-changed-while-moving = { $name } changed while it was being moved, so the original was left where it is.
# $name (String) - the folder's name, quoted
ui-files-failure-folder-in-the-way = A folder called { $name } is there already, so a file cannot replace it.
# $name (String) - the name, quoted
ui-files-failure-not-regular-file = { $name } is not a regular file, so it cannot be copied.
ui-files-failure-no-home = Your home folder could not be found.
# $path (String) - the Trash folder's path; $why (String) - the reason, a sentence
ui-files-failure-trash-folder-not-made = The Trash folder { $path } could not be made: { $why }
ui-files-failure-not-copied-into-trash = It could not be copied into the Trash.
# $path (String) - where the copy is; $why (String) - the reason, a sentence
ui-files-failure-copied-not-trashed = It was copied to { $path } but could not be put in the Trash: { $why }
# $name (String) - the name, quoted
ui-files-failure-no-record = There is no record of where { $name } came from. Use Restore to… to choose a folder.
# $name (String) - the name, quoted; $why (String) - the reason, a sentence
ui-files-failure-not-moved-to-make-room = { $name } could not be moved to the Trash to make room: { $why }
# $parent (String) - the folder; $name (String) - the name, quoted
ui-files-failure-put-back-not-folder = { $parent } is not a folder, so { $name } cannot be put back there.
ui-files-failure-put-back-folder-gone = The folder { $parent } no longer exists, so { $name } cannot be put back.
ui-files-failure-put-back-denied = You do not have permission to put { $name } back in { $parent }.
ui-files-failure-put-back-read-only = { $parent } is on a read-only drive, so { $name } cannot be put back there.
ui-files-failure-video-unreadable = The video could not be read.
ui-files-failure-no-picture = The file has no picture to reduce.
ui-files-failure-audio-unreadable = The audio could not be read.
ui-files-failure-no-sound = The file has no sound to reduce.
ui-files-failure-null-in-path = The path holds a null character.
ui-files-failure-no-folder = The file has no folder.
# $step (String) - the step that failed, a sentence lead; $detail (String) - the program's own line
ui-files-failure-tool-failed = { $step }: { $detail }.
# $step (String) - the step that failed, a sentence lead
ui-files-failure-tool-silent = { $step }: the program gave no reason.
ui-files-failure-no-output-stream = The program gave no output stream.
ui-files-failure-no-error-stream = The program gave no error stream.
ui-files-failure-not-an-archive = That is not an archive.
ui-files-failure-archive-escapes = The archive has files that would be put outside its folder, so it was not extracted.
ui-files-failure-needs-password = This archive needs a password.
ui-files-failure-wrong-password = The password is not right.
ui-files-failure-nothing-extracted = Nothing was extracted.
ui-files-failure-cannot-reduce-kind = This kind of file cannot be reduced here.
ui-files-failure-cannot-open-kind = This kind of archive cannot be opened here.
ui-files-failure-cannot-lock = This format cannot be locked with a password.
ui-files-failure-lock-needs-seven-zip = A password needs 7-Zip, which is not installed.
ui-files-failure-split-needs-program = The program for this format is not installed.
ui-files-failure-split-only-zip-and-7z = Only .zip and .7z can be cut into volumes.
# $format (String) - a picture format such as JPEG
ui-files-failure-no-picture-encoder = No { $format } encoder is installed.
ui-files-failure-no-sound-encoder = ffmpeg here has no encoder for the sound.
ui-files-failure-not-media-file = It is not a picture, a film or a song.
ui-files-failure-not-a-file = It is not a file.
# $problem (String) - what is wrong with the new file, a clause
ui-files-failure-new-file-failed-check = The new file did not pass its check: { $problem }
ui-files-failure-avif-needs-ffmpeg = AVIF needs ffmpeg, which is not installed.
# $codec (String) - a codec such as AV1 or H.264
ui-files-failure-no-codec-encoder = ffmpeg here has no { $codec } encoder.
ui-files-failure-no-picture-tools = ImageMagick and ffmpeg are not installed.
ui-files-check-cannot-open = it cannot be opened.
ui-files-check-no-picture = it has no picture.
ui-files-check-no-sound = it has no sound.
# $after (String) - seconds long now, such as 3.2; $before (String) - seconds long before
ui-files-check-too-short = it is { $after } s long, not { $before } s.

## Files: descriptive labels of the Compress and Reduce forms

# $label (Text) - the format's ending, such as .zip; $note (Text) - why to pick it
ui-compress-format-option = { $label }, { $note }
ui-compress-note-zip = works everywhere
ui-compress-note-tar-gz = fast, common on Linux and Mac
ui-compress-note-tar-xz = smaller, slower, Linux and Mac
ui-compress-note-tar-zst = fast and small, newer systems
ui-compress-note-tar-bz2 = smaller than gzip, slower
ui-compress-note-seven-zip = smallest, needs 7-Zip to open
ui-compress-note-tar = no compression, keeps permissions
ui-compress-level-fastest = Fastest
ui-compress-level-balanced = Balanced
ui-compress-level-smallest = Smallest
ui-compress-kind-pictures = Pictures
ui-compress-kind-videos = Videos
ui-compress-kind-audio = Audio
ui-compress-keep-format = Keep format
ui-compress-codec-flac = FLAC (lossless)
