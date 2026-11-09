### Bingux Settings.
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `settings-`.
###
### Search keywords (`settings_index.rs`) stay literal in the code: they are
### search data that people type, not text the window shows.

# The app's name, in the window title, the header and the About window.
settings-title = Settings
# One line in the About window.
settings-description = Change Bingux and system settings.

## Pages (sidebar, headers and search)

# The title of each page, in the sidebar, the header and search results.
settings-page-network = Network
settings-page-bluetooth = Bluetooth
settings-page-appearance = Appearance
settings-page-privacy = Privacy
settings-page-accessibility = Accessibility
settings-page-desktop = Desktop and dock
settings-page-search = Search
settings-page-displays = Displays
settings-page-sound = Sound
settings-page-power = Power
settings-page-keyboard = Keyboard
settings-page-mouse = Mouse and touchpad
settings-page-datetime = Date and time
settings-page-users = Users
settings-page-applications = Applications
settings-page-about = About

## Window shell (the sidebar, the command palette and the shortcuts sheet)

# The group of links under a page, for the pages that go with it.
settings-related-title = Related
# Commands in the palette and the shortcuts sheet.
settings-command-search = Search settings
settings-command-back = Back
settings-command-regions = Move between the sidebar and the page

## Routes (page titles in the window's title bar and history)

# Titles of subpages, shown in the header and the history menu.
routes-default-apps = Default applications
routes-time-zone = Time zone
routes-font = Font
# $number (Number) - the display's position, counting from 1.
routes-display = Display { $number }
routes-shortcuts = Shortcuts
routes-application-volumes = Application volumes
routes-wifi = Wi-Fi
routes-saved-networks = Saved networks
routes-wifi-hotspot = Wi-Fi hotspot
routes-profiles = Profiles
routes-mobile-network = Mobile network
routes-advanced = Advanced
routes-data-usage = Data usage
routes-access-point-names = Access point names
routes-new-access-point = New access point
routes-proxy = Proxy
routes-tunnel = Tunnel
# $number (Number) - the peer's position, counting from 1.
routes-peer = Peer { $number }
routes-network = Network

## Applications (launch and removal messages in apps.rs)

# $name (String) - the application's display name.
apps-launch-failed = Could not open “{ $name }”. Check that it is still installed, then try again.
apps-remove-store = Remove it from the store you installed it from.
apps-remove-nix = Remove it from your Nix profile or configuration.
apps-remove-admin = Ask an administrator to remove it.
apps-remove-system = The system installed it. Ask an administrator to remove it.
