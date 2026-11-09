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

## Password change (messages from passwd.rs)

passwd-generic = Could not change the password. Try again.
passwd-timed-out = The password change timed out. Try again.
passwd-too-weak = The new password is too weak. Use a longer password with letters, numbers and symbols.
passwd-mismatch = The new passwords do not match.
passwd-current-wrong = The current password is not correct. Check it and try again.

## Setting rows (titles shown in the rows and search; ids in settings_index.rs)

settings-setting-network-wi-fi = Wi-Fi
settings-setting-network-airplane-mode = Airplane mode
settings-setting-network-connect-to-hidden-network = Connect to hidden network
settings-setting-network-saved-networks = Saved networks
settings-setting-network-wi-fi-hotspot = Wi-Fi hotspot
settings-setting-network-wired = Wired
settings-setting-network-add-vpn = Add VPN
settings-setting-network-proxy = Proxy
settings-setting-network-data-usage = Data usage
settings-setting-network-mobile-network = Mobile network
settings-setting-network-share-wi-fi-network = Share Wi-Fi network
settings-setting-network-technical-details = Technical details
settings-setting-network-connect-automatically = Connect automatically
settings-setting-network-wi-fi-password = Wi-Fi password
settings-setting-network-priority = Priority
settings-setting-network-wired-profiles = Wired profiles
settings-setting-network-vpn = VPN
settings-setting-network-mtu = MTU
settings-setting-network-wake-on-lan = Wake on LAN
settings-setting-network-cloned-mac-address = Cloned MAC address
settings-setting-network-ipv4-method = IPv4 method
settings-setting-network-ipv6-method = IPv6 method
settings-setting-network-privacy-extensions = Privacy extensions
settings-setting-network-automatic-dns = Automatic DNS
settings-setting-network-dns-search-domains = DNS search domains
settings-setting-network-use-this-connection-only-for-resources-on-its-network = Use this connection only for resources on its network
settings-setting-network-random-mac-address = Random MAC address
settings-setting-bluetooth-bluetooth = Bluetooth
settings-setting-bluetooth-connects-automatically = Connects automatically
settings-setting-bluetooth-forget-device = Forget device
settings-setting-bluetooth-battery = Battery
settings-setting-bluetooth-name = Name
settings-setting-displays-resolution = Resolution
settings-setting-displays-refresh-rate = Refresh rate
settings-setting-displays-scale = Scale
settings-setting-displays-orientation = Orientation
settings-setting-displays-arrangement = Arrangement
settings-setting-displays-primary-display = Primary display
settings-setting-displays-enabled = Enabled
settings-setting-displays-night-light = Night light
settings-setting-displays-on-a-schedule = On a schedule
settings-setting-displays-colour-temperature = Colour temperature
settings-setting-sound-volume = Volume
settings-setting-sound-output-device = Output device
settings-setting-sound-input-device = Input device
settings-setting-sound-application-volumes = Application volumes
settings-setting-sound-effects = Effects
settings-setting-power-power-mode = Power mode
settings-setting-power-battery = Battery
settings-setting-power-automatic-screen-lock = Automatic screen lock
settings-setting-power-lock-after = Lock after
settings-setting-power-blank-screen-after = Blank screen after
settings-setting-power-dim-screen-when-inactive = Dim screen when inactive
settings-setting-power-suspend-when-inactive = Suspend when inactive
settings-setting-power-suspend-after = Suspend after
settings-setting-power-when-the-power-button-is-pressed = When the power button is pressed
settings-setting-keyboard-input-sources = Input sources
settings-setting-keyboard-keyboard-layout = Keyboard layout
settings-setting-keyboard-input-method = Input method
settings-setting-keyboard-repeat-keys = Repeat keys
settings-setting-keyboard-repeat-speed = Repeat speed
settings-setting-keyboard-delay-before-repeat = Delay before repeat
settings-setting-keyboard-cursor-blinking = Cursor blinking
settings-setting-keyboard-blink-speed = Blink speed
settings-setting-keyboard-caps-lock-key = Caps Lock key
settings-setting-keyboard-compose-key = Compose key
settings-setting-keyboard-num-lock-on-at-sign-in = Num Lock on at sign-in
settings-setting-keyboard-remember-num-lock-state = Remember Num Lock state
settings-setting-keyboard-shortcuts = Shortcuts
settings-setting-keyboard-try-it = Try it
settings-setting-mouse-pointer-acceleration = Pointer acceleration
settings-setting-mouse-left-handed-buttons = Left-handed buttons
settings-setting-mouse-middle-click-paste = Middle click paste
settings-setting-mouse-natural-scrolling = Natural scrolling
settings-setting-mouse-tap-to-click = Tap to click
settings-setting-mouse-two-finger-scrolling = Two-finger scrolling
settings-setting-mouse-scrolling-along-edges = Scrolling along edges
settings-setting-mouse-off-when-a-mouse-is-connected = Off when a mouse is connected
settings-setting-mouse-touchpad-pointer-acceleration = Touchpad pointer acceleration
settings-setting-mouse-primary-button = Primary button
settings-setting-appearance-wallpaper = Wallpaper
settings-setting-appearance-accent-colour = Accent colour
settings-setting-appearance-colour-scheme = Colour scheme
settings-setting-appearance-font = Font
settings-setting-appearance-text-size = Text size
settings-setting-accessibility-large-text = Large text
settings-setting-accessibility-high-contrast = High contrast
settings-setting-accessibility-reduce-animations = Reduce animations
settings-setting-accessibility-cursor-size = Cursor size
settings-setting-accessibility-overlay-scrollbars = Overlay scrollbars
settings-setting-accessibility-screen-reader = Screen reader
settings-setting-accessibility-on-screen-keyboard = On-screen keyboard
settings-setting-accessibility-sticky-keys = Sticky keys
settings-setting-accessibility-slow-keys = Slow keys
settings-setting-accessibility-bounce-keys = Bounce keys
settings-setting-accessibility-mouse-keys = Mouse keys
settings-setting-accessibility-locate-pointer = Locate pointer
settings-setting-appearance-icons = Icons
settings-setting-appearance-cursor = Cursor
settings-setting-desktop-show-dock = Show dock
settings-setting-desktop-icon-size = Icon size
settings-setting-desktop-side-of-the-screen = Side of the screen
settings-setting-desktop-alignment = Alignment
settings-setting-desktop-dock-mouse-actions = Dock mouse actions
settings-setting-desktop-click = Click
settings-setting-desktop-middle-click = Middle click
settings-setting-desktop-scroll = Scroll
settings-setting-desktop-scroll-direction = Scroll direction
settings-setting-desktop-show-sidebar = Show sidebar
settings-setting-desktop-show-seconds-in-the-clock = Show seconds in the clock
settings-setting-desktop-show-system-usage = Show system usage
settings-setting-desktop-show-my-public-ip-address = Show my public IP address
settings-setting-desktop-control-centre = Control Centre
settings-setting-desktop-top-bar = Top bar
settings-setting-search-search-sources = Search sources
settings-setting-search-file-search = File search
settings-setting-search-new-folder = New folder
settings-setting-search-search-subfolders = Search subfolders
settings-setting-search-previews = Previews
settings-setting-search-preview-files = Preview files
settings-setting-search-largest-file-to-preview = Largest file to preview
settings-setting-search-prepare-nearby-previews = Prepare nearby previews
settings-setting-search-web-search = Web search
settings-setting-search-default-engine = Default engine
settings-setting-search-new-search-engine = New search engine
settings-setting-search-assistant = Assistant
settings-setting-search-assistant-program = Assistant program
settings-setting-search-model = Model
settings-setting-applications-default-applications = Default applications
settings-setting-applications-open-by-default = Open by default
settings-setting-applications-installed = Installed
settings-setting-applications-start-at-login = Start at login
settings-setting-applications-file-types = File types
settings-setting-applications-notifications = Notifications
settings-setting-datetime-set-automatically = Set automatically
settings-setting-datetime-time-zone = Time zone
settings-setting-datetime-clock-format = Clock format
settings-setting-datetime-show-seconds = Show seconds
settings-setting-datetime-show-date-in-the-top-bar = Show date in the top bar
settings-setting-datetime-show-weekday-in-the-top-bar = Show weekday in the top bar
settings-setting-datetime-set-date-and-time = Set date and time
settings-setting-users-full-name = Full name
settings-setting-users-user-name = User name
settings-setting-users-account-type = Account type
settings-setting-users-change-password = Change password
settings-setting-users-automatic-login = Automatic login
settings-setting-users-account-locked = Account locked
settings-setting-users-language = Language
settings-setting-about-device-name = Device name
settings-setting-about-hardware-model = Hardware model
settings-setting-about-processor = Processor
settings-setting-about-memory = Memory
settings-setting-about-graphics = Graphics
settings-setting-about-capacity = Capacity
settings-setting-about-kernel = Kernel
settings-setting-about-operating-system = Operating system
settings-setting-about-desktop = Desktop
settings-setting-about-display = Display
settings-setting-about-version = Version

## Application sources (labels in lists, pages and removal messages)

settings-source-local = Local desktop entry
settings-source-system = System
settings-source-rpm = RPM package
