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

ui-window-minimise = Minimise
ui-window-maximise = Maximise
ui-window-restore = Restore
# Tooltip and screen reader name of the window's close button.
ui-window-close-button = Close
# Tooltip of the spinner beside a window title while the app works.
ui-window-busy = Working…
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
ui-validate-ssid-empty = Enter the network name.
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
