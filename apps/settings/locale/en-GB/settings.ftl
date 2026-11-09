### Bingux Settings.
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `settings-`.
###
### Search keywords (`settings_index.rs`) stay literal in the code: they are
### search data that people type, not text the window shows.
### Fixture data (the snapshot-only fake Bluetooth model) stays literal: it is
### test input, never shown to a real user.

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

## Power

power-battery-time-empty = Time to empty
power-battery-time-full = Time to full
power-battery-unknown = Unknown
power-battery-waiting-to-charge = Waiting to charge
power-battery-waiting-to-discharge = Waiting to discharge
power-battery-group = Battery
power-battery-ring-label = Charge { $percentage }, { $state }
power-battery-charge = Charge
power-battery-state = State
power-battery-health = Battery health
power-battery-chart-a11y = Battery charge over the last 24 hours
power-history-group = Last 24 hours
power-show-percentage = Show battery percentage
power-plugged-in = Plugged in
power-profile-performance = Performance
power-profile-balanced = Balanced
power-profile-power-saver = Power saver
power-profile-performance-meaning = Fastest. Uses more power and runs warmer.
power-profile-balanced-meaning = Good for most work.
power-profile-power-saver-meaning = Uses less power. Slower under heavy load.
power-mode-group = Power mode
power-mode-unavailable = Not available
power-mode-in-use = In use
power-modes-unavailable = Power modes are not available on this computer.
power-setting-read-error = Could not read this setting. Try again in a moment.
power-write-failed = Could not change the power setting. Try again.
power-try-again = Try again
power-suspend-after = Suspend after
power-suspend-when-inactive = Suspend when inactive
power-screen-group = Screen
power-blank-screen-after = Blank screen after
power-dim-screen = Dim screen when inactive
power-lock-after = Lock after
power-lock-immediately = Immediately
power-auto-lock = Automatic screen lock
power-never = Never
power-when-plugged-in = When plugged in
power-on-battery = On battery
power-button-group = Power button
power-when-pressed = When pressed
power-button-nothing = Do nothing
power-button-suspend = Suspend
power-button-hibernate = Hibernate
power-button-interactive = Ask what to do
power-button-shutdown = Power off

## Displays

displays-group = Displays
displays-arrangement = Arrangement
displays-night-light = Night light
displays-use-night-light = Use night light
displays-colour-temperature = Colour temperature
displays-on-a-schedule = On a schedule
displays-from = From
displays-until = Until
displays-apply = Apply
displays-apply-hint = After you apply, choose whether to keep the new layout.
displays-identify = Identify
displays-try-again = Try again
displays-open-details = Open display details
displays-read-only-now = This display is read-only right now.
displays-no-longer-connected = This display is no longer connected.
displays-read-only-reason = Display settings are read-only right now. The list shows the connected displays.
displays-read-error = Could not read the display settings. Try again in a moment.
displays-night-read-error = Could not read the night light settings. Try again in a moment.
displays-night-write-error = Could not change night light. Try again.
displays-apply-failed = Could not apply the display layout. Try again.
displays-preview-apply-failed = Could not apply the preview layout. Restart Settings without the test layout.
displays-save-failed = Could not save the display layout. Try again.
displays-restore-failed = Could not restore the previous layout. Try again.
displays-layout-changed = The display layout changed. Make your changes again.
displays-move-failed = Could not move the display. Drop it next to another display and try again.
displays-keep-one-on = Keep at least one display on.
displays-turn-on-one = Turn on at least one display.
displays-choose-primary = Choose one primary display.
displays-turn-on-to-make-primary = Turn on this display to make it the primary display.
displays-no-displays-on = No displays are on.
displays-no-mode-for-resolution = No mode has that resolution.
displays-unknown-refresh-rate = Unknown refresh rate.
displays-unknown-scale = Unknown scale.
displays-unknown-orientation = Unknown orientation.
displays-unknown = Unknown
displays-off = Off
displays-yes = Yes
displays-no = No
displays-port = Port
displays-in-use = In use
displays-primary = Primary
displays-mode = Mode
displays-position = Position
displays-scale = Scale
displays-orientation = Orientation
displays-resolution = Resolution
displays-refresh-rate = Refresh rate
displays-use-this-display = Use this display
displays-state-off = Off
displays-state-primary = Primary display
displays-state-secondary = Secondary display
displays-drag-to-arrange = Drag displays to arrange them.
# $x (Number) - the display's horizontal position in pixels
# $y (Number) - the display's vertical position in pixels
displays-position-value = Position { NUMBER($x) }, { NUMBER($y) }
displays-keep-title = Keep these display settings?
displays-keep-body = The new layout is active for now.
displays-revert = Revert
displays-keep = Keep
# $seconds (Number) - seconds left before the layout reverts
displays-reverting-countdown = { $seconds ->
    [one] Reverting in 1 second…
   *[other] Reverting in { NUMBER($seconds) } seconds…
}
displays-make-primary = Make primary
displays-rotate = Rotate
displays-turn-off = Turn off
displays-settings-ellipsis = Settings…
displays-temperature-very-warm = Very warm
displays-temperature-warm = Warm
displays-temperature-neutral = Neutral
displays-temperature-cool = Cool
displays-orientation-normal = Normal
displays-orientation-90 = 90°
displays-orientation-180 = 180°
displays-orientation-270 = 270°
displays-orientation-flipped = Flipped
displays-orientation-flipped-90 = Flipped 90°
displays-orientation-flipped-180 = Flipped 180°
displays-orientation-flipped-270 = Flipped 270°

## Sound

sound-group = Sound
sound-kind-output = Output
sound-kind-input = Input
sound-kind-none-output = No output devices found
sound-kind-none-input = No input devices found
sound-kind-device-output = Output device
sound-kind-device-input = Input device
sound-kind-volume-output = Output volume
sound-kind-volume-input = Input volume
sound-playing-to = Playing to { $devices }
sound-recording-from = Recording from { $devices }
sound-paused = Paused
sound-effects-start-failed = Could not start the effects. Try again.
sound-change-failed = Could not change the sound. Try again.
sound-volume-change-failed = Could not change the volume. Try again.
sound-mix-balance = Balance
sound-mix-fade = Fade
sound-mix-sub = Subwoofer
sound-side-left = Left
sound-side-right = Right
sound-side-rear = Rear
sound-side-front = Front
sound-centre = Centre
sound-channel-front-left = Front left
sound-channel-front-right = Front right
sound-channel-centre = Centre
sound-channel-subwoofer = Subwoofer
sound-channel-rear-left = Rear left
sound-channel-rear-right = Rear right
sound-channel-side-left = Side left
sound-channel-side-right = Side right
sound-test-tone-disk-failed = Could not play the test sound. Free up disk space and try again.
sound-test-tone-failed = Could not play the test sound. Check that sound is working and try again.
sound-choose-speaker = Choose a speaker
sound-test-speakers = Test speakers
sound-unmute = Unmute
sound-mute = Mute
sound-use-as-default = Use { $name } as default
sound-test-ellipsis = Test…
sound-allow-above-100 = Allow volume above 100%
sound-play-to-device = Play to { $device }
sound-record-from-device = Record from { $device }
sound-device-no-name = The device has no name.
sound-move-failed = Could not move the sound to that device. Try again.
sound-reset-volume = Reset to 100%
sound-stopped = Stopped
sound-fixed-volume = Fixed volume
sound-summary-idle = Nothing playing
sound-summary-playing = { $count } playing
sound-summary-recording = { $count } recording
sound-summary-both = { $playing } playing, { $recording } recording
sound-effects-output-failed = Could not change output effects. Try again.
sound-try-again = Try again
sound-application-volumes = Application volumes
sound-effects = Effects
sound-off = Off
sound-output-effects = Output effects
sound-applications-empty = Nothing playing or recording
sound-meter-output-level = Output level
sound-meter-input-level = Input level
sound-meter-application-level = Application level
sound-eq-mode = Mode
sound-eq-mid = Mid
sound-eq-side = Side
sound-eq-channel = Channel
sound-eq-band-gain = Band { $number } gain
sound-eq-edit-band = Edit band { $number }
sound-eq-add-band = Add band
sound-eq-remove-band = Remove band
sound-eq-flat-response = Flat response
sound-eq-bands = Bands
sound-eq-type = Type
sound-eq-frequency = Frequency
sound-eq-quality-factor = Quality factor
sound-eq-slope = Slope
sound-eq-mute = Mute
sound-eq-gain = Gain
sound-eq-band-title = Band { $number }

# Presets menu and dialogs. "Flat" and "Imported" are stored preset names, not UI text, so they stay literal.
sound-action-cancel = Cancel
sound-action-close = Close
sound-file-filter-all = All files
sound-preset-menu-compact = Presets
# $name (String) - the name of the preset in use
sound-preset-menu = Preset: { $name }
sound-preset-new = New preset…
sound-preset-new-title = New preset
sound-preset-create = Create
sound-preset-duplicate = Duplicate
sound-preset-rename = Rename…
sound-preset-rename-title = Rename preset
sound-preset-import = Import…
sound-preset-import-title = Import preset
sound-preset-filter = Presets
sound-preset-export = Export…
sound-preset-export-title = Export preset
sound-preset-delete = Delete
# $name (String) - the name of the preset being deleted
sound-preset-delete-title = Delete “{ $name }”?
sound-preset-delete-warning = You cannot undo this.
# $name (String) - the name the user typed that is already taken
sound-preset-exists = A preset called “{ $name }” already exists. Choose another name.
sound-preset-switch-failed = Could not switch preset. Try again.
sound-preset-change-failed = Could not change the preset. Try again.
# $error (String) - the underlying error text
sound-preset-read-failed = Could not read the file. { $error }
# $name (String) - the imported preset name
sound-preset-imported = “{ $name }” imported
# $lines (String) - one line per setting that did not map exactly
sound-preset-import-approximate = Some settings have no exact match. They were left out or approximated:
    { $lines }
sound-preset-import-failed = Could not import the preset. Check that the file is a preset, then try again.
sound-preset-exported = Preset exported
sound-preset-export-failed = Could not export the preset. Check the folder, then try again.

# Effects chain picker and tabs.
sound-effects-done = Done
sound-effects-tab-output = Output
sound-effects-tab-input = Input
sound-effects-add-title = Add effect
sound-effects-search-placeholder = Search effects
sound-effects-looking = Looking for effects…
# $package (String) - the Fedora package name to install
sound-effects-not-installed = Not installed. Install { $package }.
sound-effects-none-match = No matching effects

# Effects chain stack and device rows.
sound-effects-not-running = It is not running.
sound-effects-save-failed = Could not save the effects. Try again.
sound-effects-change-failed = Could not change the effects. Try again.
sound-effects-empty = No effects added
sound-effects-remove = Remove effect
sound-effects-off = Off
sound-effects-add-button = Add effect
sound-default-speakers = Speakers
sound-default-microphone = Microphone
sound-meter-in = In
sound-meter-out = Out
# Effect page (apps/settings/src/pages/sound/effects/page.rs)
sound-effect-add-prompt = Add an effect to the stack.
sound-effect-unavailable = This effect is not available.
sound-effect-reset = Reset
sound-effect-reading = Reading the effect…
# $name (String) - the effect's display name
# $package (String) - the Fedora package that provides the effect
sound-effect-not-installed-full = { $name } is not installed. Install { $package } to use it.
# $project (String) - the upstream project that makes the effect
sound-effect-using = Using { $project }
sound-effect-file = File
sound-effect-none = None
sound-effect-choose = Choose…
sound-effect-choose-title = Choose an impulse response
sound-effect-audio-files = Audio files
sound-effect-group-compression = Compression
sound-effect-group-timing = Timing
sound-effect-group-mix = Mix
sound-effect-group-side-chain = Side-chain
sound-effect-group-limit = Limit
sound-effect-group-level-regulation = Level regulation
sound-effect-group-quality = Quality
sound-effect-group-gate = Gate
sound-effect-group-hysteresis = Hysteresis
sound-effect-group-expansion = Expansion
sound-effect-group-controls = Controls
sound-effect-group-room = Room
sound-effect-group-tone = Tone
sound-effect-group-left = Left
sound-effect-group-right = Right
sound-effect-group-filter = Filter
sound-effect-group-mode = Mode
sound-effect-group-gain = Gain
sound-effect-group-input-gain = Input gain
sound-effect-group-output-gain = Output gain
sound-effect-group-impulse-response = Impulse response
sound-effect-group-options = Options
sound-effect-group-modes = Modes
sound-effect-group-level = Level
sound-effect-group-frequency = Frequency
sound-effect-group-settings = Settings

## Date and time

# Error text for reads and writes of the clock, shown as a status or toast.
datetime-read-timezone-error = Could not read the time zone. Try again in a moment.
datetime-read-clock-error = Could not read the clock setting. Try again in a moment.
datetime-read-time-error = Could not read the time. Try again in a moment.
datetime-set-time-error = Could not set the time. Try again.
datetime-auto-time-error = Could not change automatic time. Try again.
datetime-clock-format-error = Could not change the clock format. Try again.
datetime-seconds-error = Could not change the seconds setting. Try again.
datetime-weekday-error = Could not change the weekday setting. Try again.
datetime-date-setting-error = Could not change the date setting. Try again.
datetime-auto-zone-error = Could not change automatic time zone. Try again.
datetime-week-numbers-error = Could not change the week numbers setting. Try again.
datetime-set-zone-error = Could not set the time zone. Try again.
datetime-copy-zone-error = Could not copy the time zone
datetime-zone-copied = Time zone copied
datetime-set-date-error = Could not set the date and time. Try again.
datetime-try-again = Try again
# The clock settings page and its set-date sheet.
datetime-set-date-title = Set date and time
datetime-set-date-menu = Set date and time…
datetime-date-group = Date
datetime-day = Day
datetime-month = Month
datetime-year = Year
datetime-time-group = Time
datetime-hour = Hour
datetime-minute = Minute
datetime-set-action = Set
datetime-setting-date = Setting the date and time…
datetime-month-january = January
datetime-month-february = February
datetime-month-march = March
datetime-month-april = April
datetime-month-may = May
datetime-month-june = June
datetime-month-july = July
datetime-month-august = August
datetime-month-september = September
datetime-month-october = October
datetime-month-november = November
datetime-month-december = December
datetime-turn-on = Turn on
datetime-turn-off = Turn off
datetime-time-zone = Time zone
datetime-change-zone = Change time zone
datetime-copy-zone = Copy time zone
datetime-set-zone = Set time zone
datetime-clock = Clock
datetime-clock-format = Clock format
datetime-format-24 = 24-hour
datetime-format-12 = 12-hour
datetime-use-24-hour = Use 24-hour
datetime-use-12-hour = Use 12-hour
datetime-set-automatically = Set automatically
datetime-set-zone-automatically = Set time zone automatically
datetime-format-group = Format
datetime-show-seconds = Show seconds
datetime-show-weekday = Show weekday in the top bar
datetime-show-date = Show date in the top bar
datetime-show-week-numbers = Show week numbers in the calendar
datetime-no-matching-cities = No matching cities
# $count (Number) - how many zones are hidden below the list
datetime-more-zones = { NUMBER($count) } more. Type to narrow the list.
datetime-city-placeholder = City
# $time (String) - the local time in the zone, such as 09:05
datetime-local-time = Local time { $time }
datetime-save-error = Could not save the change. Try again.
datetime-am = AM
datetime-pm = PM
# $weekday (String) - the weekday name, such as Friday; $day (Number) - day of the month; $month (String) - the month name; $year (Number) - the year
datetime-date-line = { $weekday }, { $day } { $month } { $year }


## About

# The system facts in the About page, with their labels.
settings-about-spec-os = Operating system
settings-about-spec-version = Version
settings-about-spec-kernel = Kernel
settings-about-spec-processor = Processor
settings-about-spec-memory = Memory
settings-about-spec-graphics = Graphics
settings-about-spec-display = Display
settings-about-spec-uptime = Uptime
settings-about-spec-desktop = Desktop
settings-about-spec-settings-version = Settings version
# A value a fact could not be read for.
settings-about-unknown = Unknown
settings-about-not-running = Not running
# A memory size in gibibytes, such as "15.6 GiB".
settings-about-memory-gib = { $gib } GiB
# The first connected display's mode, and how many more are connected.
settings-about-display-more = { $first } (+{ $more } more)
# The text copied by "Copy system info".
settings-about-copy-device-name = Device name: { $name }
settings-about-copy-model = Hardware model: { $model }
settings-about-copy-row = { $title }: { $value }
settings-about-copy-storage = Storage: { $used } used of { $total }
settings-about-copy-storage-unknown = Storage: Unknown
# Toasts after copying the system info.
settings-about-copied = Copied
settings-about-copy-failed = Could not copy to the clipboard. Try again.
# The device name row, its rename sheet and its validation messages.
settings-about-device-name = Device name
settings-about-rename = Rename
settings-about-name-empty = Enter a device name.
settings-about-name-too-long = Use 63 characters or fewer.
settings-about-name-invalid = Use only letters, numbers and hyphens. Do not start or end with a hyphen.
settings-about-name-help = Other devices on your network see this name. Use letters, numbers and hyphens.
# The storage gauge.
settings-about-storage = Storage
settings-about-storage-used-unit = { $unit } used
settings-about-storage-total = { $total } total
settings-about-storage-used-of = { $used } used of { $total }
# Labels and menu items for the rest of the page.
settings-about-capacity = Capacity
settings-about-copy = Copy
settings-about-copy-system-info = Copy system info
settings-about-system-details = System details

## Accessibility

# The groups of switches on the Accessibility page, and their labels.
settings-accessibility-seeing = Seeing
settings-accessibility-screen-reader = Screen reader
settings-accessibility-large-text = Large text
settings-accessibility-high-contrast = High contrast
settings-accessibility-reduce-animations = Reduce animations
settings-accessibility-cursor-size = Cursor size
settings-accessibility-overlay-scrollbars = Overlay scrollbars
settings-accessibility-typing = Typing
settings-accessibility-on-screen-keyboard = On-screen keyboard
settings-accessibility-sticky-keys = Sticky keys
settings-accessibility-slow-keys = Slow keys
settings-accessibility-bounce-keys = Bounce keys
settings-accessibility-pointing = Pointing
settings-accessibility-mouse-keys = Mouse keys
settings-accessibility-locate-pointer = Locate pointer

## Appearance

# The wallpaper, font, accent and colour scheme on the Appearance page.
settings-appearance-text = Text
settings-appearance-font = Font
settings-appearance-text-size = Text size
settings-appearance-themes = Themes
settings-appearance-icons = Icons
settings-appearance-cursor = Cursor
settings-appearance-accent-colour = Accent colour
settings-appearance-accent-label = Accent colour { $name }
settings-appearance-colour-scheme = Colour scheme
settings-appearance-scheme-default = Default
settings-appearance-scheme-light = Light
settings-appearance-scheme-dark = Dark
settings-appearance-gsettings-unavailable = Accent colour and colour scheme are not available on this system.
settings-appearance-wallpaper = Wallpaper
settings-appearance-choose-wallpaper = Choose a wallpaper
settings-appearance-filter-pictures = Pictures
settings-appearance-choose-picture = Choose picture
settings-appearance-use-system-wallpaper = Use system wallpaper
settings-appearance-system-wallpaper = System wallpaper
settings-appearance-add-picture = Add picture…
settings-appearance-search-pictures = Search pictures
settings-appearance-source-system = System
settings-appearance-source-yours = Your pictures
settings-appearance-no-pictures = No pictures. Add one to use it as the wallpaper.
settings-appearance-no-matching-pictures = No matching pictures
settings-appearance-font-not-installed = Could not set the font “{ $family }”. Choose an installed font.
settings-appearance-try-again = Try again
settings-appearance-system-default = System default
settings-appearance-font-name-copied = Font name copied
settings-appearance-font-copy-failed = Could not copy the font name. Try again.
settings-appearance-use-this-font = Use this font
settings-appearance-copy-name = Copy name
settings-appearance-no-matching-fonts = No matching fonts
settings-appearance-filter-all = All
settings-appearance-filter-sans-serif = Sans serif
settings-appearance-filter-serif = Serif
settings-appearance-filter-monospace = Monospace
settings-appearance-filter-other = Other
settings-appearance-search-fonts = Search fonts

## Applications

# Default applications, the installed list and the uninstall dialog.
settings-applications-kind-web = Web
settings-applications-kind-mail = Mail
settings-applications-kind-calendar = Calendar
settings-applications-kind-files = Files
settings-applications-kind-text = Text
settings-applications-kind-documents = Documents
settings-applications-kind-photos = Photos
settings-applications-kind-music = Music
settings-applications-kind-video = Video
settings-applications-uninstalled = “{ $name }” uninstalled
settings-applications-uninstall-failed = Could not uninstall “{ $name }”. Try again.
settings-applications-uninstall-body-source = “{ $name }” is installed from { $source }.
settings-applications-uninstall-body-system = “{ $name }” is installed on this system.
settings-applications-uninstall-title = Uninstall “{ $name }”?
settings-applications-cancel = Cancel
settings-applications-uninstall = Uninstall
settings-applications-not-set = Not set
settings-applications-open = Open
settings-applications-show-details = Show details
settings-applications-make-default = Make default
settings-applications-search = Search
settings-applications-filter-all = All
settings-applications-sort-name = Name
settings-applications-sort-size = Size
settings-applications-filter-and-sort = Filter and sort
settings-applications-source = Source
settings-applications-sort-by = Sort by
settings-applications-installed = Installed
settings-applications-showing = Showing { $kind }
settings-applications-largest-first = Largest first
settings-applications-showing-largest-first = Showing { $kind }, largest first
settings-applications-no-matching = No matching applications
settings-applications-default-applications = Default applications
settings-applications-unknown = Unknown
settings-applications-no-longer-installed = This application is no longer installed.
settings-applications-open-by-default = Open by default
settings-applications-try-again = Try again
settings-applications-not-default-application = Not available as a default application.
settings-applications-default = Default
settings-applications-not-default = Not default
settings-applications-details = Details
settings-applications-version = Version
settings-applications-size = Size
settings-applications-installed-from = Installed from
settings-applications-location = Location
settings-applications-category = Category
settings-applications-startup = Startup
settings-applications-start-at-login = Start at login
settings-applications-notifications = Notifications
settings-applications-show-notifications = Show notifications
settings-applications-show-on-lock-screen = Show on the lock screen
settings-applications-play-notification-sounds = Play notification sounds
settings-applications-removes-package = Removes the package this application came from.
settings-applications-deletes-files = Deletes the files this application installed.
settings-applications-remove-from-computer = Remove from this computer
settings-applications-file-types = File types
settings-applications-supported-file-types = Supported file types
settings-applications-other-file-types = Other file types
settings-applications-could-not-find-file-type = Could not find this file type.
settings-applications-default-application = Default application
settings-applications-has-default = Has a default
settings-applications-sort-application = Application
settings-applications-no-matching-file-types = No matching file types
settings-applications-category-images = Images
settings-applications-category-audio = Audio
settings-applications-category-video = Video
settings-applications-category-documents = Documents
settings-applications-category-archives = Archives
settings-applications-category-fonts = Fonts
settings-applications-category-code = Code
settings-applications-category-other = Other

## Desktop

# The Desktop and dock page: removable media, privacy, dock, sidebar and Control Centre.
settings-desktop-tile-vpn = VPN
settings-desktop-tile-dnd = Do not disturb
settings-desktop-tile-night-light = Night light
settings-desktop-tile-power-mode = Power mode
settings-desktop-tile-keep-awake = Keep awake
settings-desktop-unreadable = Could not read the desktop settings. Restart the desktop and try again.
settings-desktop-removable-media = Removable media
settings-desktop-mount-when-plugged = Mount drives when they are plugged in
settings-desktop-open-after-mount = Open the folder after mounting
settings-desktop-run-programs = Run programs from removable media
settings-desktop-privacy = Privacy
settings-desktop-show-public-ip = Show my public IP address
settings-desktop-show-on-sign-in = Show on sign-in screen
settings-desktop-picture-and-wallpaper = Picture and wallpaper
settings-desktop-remember-recent = Remember recent files
settings-desktop-forget-recent-after = Forget recent files after
settings-desktop-introduction = Introduction
settings-desktop-shortcuts-tour = Shortcuts and shell tour
settings-desktop-shown-at-sign-in = Shown at first sign-in
settings-desktop-show-introduction = Show introduction
settings-desktop-introduction-failed = Could not show the introduction. Restart the desktop and try again.
settings-desktop-recent-never = Never
settings-desktop-recent-one-day = 1 day
settings-desktop-recent-days = { $count } days
settings-desktop-icon-size = Icon size
settings-desktop-icon-size-px = { $px } px
settings-desktop-try-again = Try again
settings-desktop-dock = Dock
settings-desktop-show-dock = Show dock
settings-desktop-alignment = Alignment
settings-desktop-align-left = Left
settings-desktop-align-centre = Centre
settings-desktop-align-right = Right
settings-desktop-align-left-menu = Align left
settings-desktop-align-centre-menu = Align centre
settings-desktop-align-right-menu = Align right
settings-desktop-sidebar = Sidebar
settings-desktop-show-sidebar = Show sidebar
settings-desktop-sidebar-side = Side of the screen
settings-desktop-sidebar-auto = Automatic
settings-desktop-sidebar-left = Left
settings-desktop-sidebar-right = Right
settings-desktop-sidebar-top = Top
settings-desktop-dock-mouse-actions = Dock mouse actions
settings-desktop-click = Click
settings-desktop-click-toggle = Show or hide windows
settings-desktop-click-focus = Bring the window to the front
settings-desktop-click-launch = Open a new window
settings-desktop-middle-click = Middle click
settings-desktop-middle-launch = Open a new window
settings-desktop-middle-close = Close all its windows
settings-desktop-middle-none = Do nothing
settings-desktop-scroll = Scroll
settings-desktop-scroll-cycle = Switch between windows
settings-desktop-scroll-none = Do nothing
settings-desktop-scroll-direction = Scroll direction
settings-desktop-scroll-natural = Like a touchscreen
settings-desktop-scroll-reverse = Reversed
settings-desktop-top-bar = Top bar
settings-desktop-show-system-usage = Show system usage
settings-desktop-show-clock-seconds = Show seconds in the clock
settings-desktop-control-centre = Control Centre

## Settings Leftovers

apps-category-internet = Internet
apps-category-email = Email
apps-category-office = Office
apps-category-graphics = Graphics
apps-category-music-audio = Music and audio
apps-category-video = Video
apps-category-media = Media
apps-category-development = Development
apps-category-games = Games
apps-category-education = Education
apps-category-science = Science
apps-category-system = System
apps-category-utilities = Utilities
sound-volume-read-error = Could not read the volume. Try again in a moment.


## Gnoblin

settings-gnoblin-not-running = The desktop is not running. Sign out and sign in again.
settings-gnoblin-managed = Set in your desktop configuration
settings-gnoblin-managed-badge = Managed
settings-gnoblin-on = On
settings-gnoblin-off = Off
settings-gnoblin-custom = Custom
settings-gnoblin-millis = { $ms } ms
settings-gnoblin-read-error = Could not read the desktop settings. Restart the desktop and try again.

## Keyboard

settings-keyboard-caps-default = Default
settings-keyboard-caps-escape = Works as Escape
settings-keyboard-caps-control = Works as Control
settings-keyboard-caps-off = Turned off
settings-keyboard-caps-lock = Caps Lock key
settings-keyboard-compose-off = Off
settings-keyboard-compose-ralt = Right Alt
settings-keyboard-compose-rctrl = Right Ctrl
settings-keyboard-compose-menu = Menu key
settings-keyboard-compose-key = Compose key
settings-keyboard-alt-default = Default
settings-keyboard-alt-left-alt = Left Alt
settings-keyboard-alt-right-alt = Right Alt
settings-keyboard-alt-left-super = Left Super
settings-keyboard-alt-right-super = Right Super
settings-keyboard-alternate-key = Alternate characters key
settings-keyboard-read-error = Could not read the keyboard settings. Restart Settings and try again.
settings-keyboard-open-error = Could not open the file. Check that a text editor is installed.
settings-keyboard-add-title = Add input source
settings-keyboard-all-added = All input sources are in the list.
settings-keyboard-no-matches = No matching input sources
settings-keyboard-available = Available
settings-keyboard-more-hidden = { $count } more not shown. Type to narrow the list.
settings-keyboard-kind-layout = Keyboard layout
settings-keyboard-kind-method = Input method
settings-keyboard-move-up = Move up
settings-keyboard-move-down = Move down
settings-keyboard-remove = Remove
settings-keyboard-group-windows = Windows
settings-keyboard-not-set = Not set
settings-keyboard-switch-input = Switch input source
settings-keyboard-shortcuts-read-error = Could not read the shortcuts. Restart the desktop and try again.
settings-keyboard-try-again = Try again
settings-keyboard-repeat-keys = Repeat keys
settings-keyboard-delay-before-repeat = Delay before repeat
settings-keyboard-repeat-speed = Repeat speed
settings-keyboard-try-it = Try it
settings-keyboard-key-repeat = Key repeat
settings-keyboard-typing = Typing
settings-keyboard-remember-numlock = Remember Num Lock state
settings-keyboard-numlock-sign-in = Num Lock on at sign-in
settings-keyboard-blink-speed = Blink speed
settings-keyboard-cursor-blinking = Cursor blinking
settings-keyboard-special-entry = Special character entry
settings-keyboard-input-sources = Input sources
settings-keyboard-separate-per-window = Separate input per window
settings-keyboard-add-source = Add input source…
settings-keyboard-no-sources = No input sources
settings-keyboard-shortcuts = Keyboard shortcuts
settings-keyboard-shortcuts-group = Shortcuts
settings-keyboard-no-shortcuts = No matching shortcuts
settings-keyboard-configuration = Configuration
settings-keyboard-config-file = Configuration file
settings-keyboard-open = Open
settings-keyboard-source-gone = This input source is no longer set up.
settings-keyboard-details = Details
settings-keyboard-type = Type
settings-keyboard-position = Position
settings-keyboard-actions = Actions
settings-keyboard-err-add = Could not add that input source. Choose a different one.
settings-keyboard-err-duplicate = That input source is already in the list.
settings-keyboard-err-missing = Could not find that input source. Reopen the page and try again.
settings-keyboard-err-last = Keep at least one input source.
settings-keyboard-err-first = Could not move that input source. It is already first.
settings-keyboard-err-last-move = Could not move that input source. It is already last.
settings-keyboard-err-unsupported = Could not change the input sources. Edit the configuration file instead.
settings-keyboard-err-unreadable = Could not read the configuration file. Check that it exists, then try again.
settings-keyboard-err-unwritable = Could not save the configuration file. Check that you can write to it, then try again.
settings-keyboard-err-undone = The desktop did not accept the change, so it was undone. Try again.

## Bluetooth

# Error text for a failed Bluetooth action, in words for a person.
bluetooth-error-unavailable = Bluetooth is not available. Restart the computer and try again.
bluetooth-error-no-answer = Bluetooth did not answer. Try again in a moment.
bluetooth-error-busy = Bluetooth is busy. Try again in a moment.
bluetooth-error-pairing-rejected = The device did not accept the pairing request. Try pairing again.
bluetooth-error-out-of-range = The device is out of range or turned off. Turn it on, then try again.
bluetooth-error-timeout = The device did not respond in time. Bring it closer and try again.
bluetooth-error-no-response = The device did not respond. Turn it on, bring it closer and try again.
bluetooth-error-generic = Could not finish the action. Try again.
bluetooth-error-try-later = Try again in a moment.
bluetooth-error-status-read = Could not read the Bluetooth status. Try again in a moment.

# The toast when an action is requested while another one runs.
bluetooth-toast-busy = Another action is still running
# $failed (String) - the failure headline, e.g. "Could not connect to “Keyboard”."
# $reason (String) - the plain reason from the list above.
bluetooth-toast-failed = { $failed } { $reason }
bluetooth-action-retry = Try again

bluetooth-this-computer = This computer

bluetooth-busy-power-on = Turning Bluetooth on…
bluetooth-busy-power-off = Turning Bluetooth off…
bluetooth-failed-power-on = Could not turn on Bluetooth. Restart the computer and try again.
bluetooth-failed-power-off = Could not turn off Bluetooth. Try again.

# $name (String) - the device's name.
bluetooth-toast-alias-reset = “{ $name }” uses its own name again
# $alias (String) - the new name.
bluetooth-toast-renamed = Renamed to “{ $alias }”
bluetooth-busy-rename = Renaming…
# $name (String) - the device's name.
bluetooth-failed-rename = Could not rename “{ $name }”.
bluetooth-busy-save = Saving…
bluetooth-failed-trust = Could not change automatic connection for “{ $name }”. Try again.
bluetooth-busy-forget = Forgetting…
bluetooth-toast-forgot = Forgot “{ $name }”
bluetooth-failed-forget = Could not forget “{ $name }”.
bluetooth-busy-disconnect = Disconnecting…
bluetooth-toast-disconnected = Disconnected from “{ $name }”
bluetooth-failed-disconnect = Could not disconnect from “{ $name }”.
bluetooth-busy-connect = Connecting…
bluetooth-toast-connected = Connected to “{ $name }”
bluetooth-failed-connect = Could not connect to “{ $name }”.
bluetooth-busy-pair = Pairing…
bluetooth-failed-pair = Could not set up “{ $name }”.

# The line under the header title, while Bluetooth is on.
# $alias (String) - the name this computer shows to other devices.
bluetooth-header-visible = Visible as { $alias }
bluetooth-header-off = Off

# The kind of a device, from its BlueZ icon name.
bluetooth-kind-headphones = Headphones
bluetooth-kind-speakers = Speakers
bluetooth-kind-keyboard = Keyboard
bluetooth-kind-mouse = Mouse
bluetooth-kind-tablet = Tablet
bluetooth-kind-game-controller = Game controller
bluetooth-kind-phone = Phone
bluetooth-kind-computer = Computer
bluetooth-kind-display = Display
bluetooth-kind-printer = Printer
bluetooth-kind-camera = Camera
bluetooth-kind-other = Other

# The state of a device.
bluetooth-state-connected = Connected
bluetooth-state-not-connected = Not connected
bluetooth-state-not-paired = Not paired
# $state (String) - the state, e.g. "Connected".
# $level (Number) - battery percentage.
bluetooth-state-battery = { $state } · { NUMBER($level) }% battery
# $level (Number) - battery percentage.
bluetooth-battery-percent = { NUMBER($level) }%

bluetooth-action-forget = Forget
bluetooth-action-disconnect = Disconnect
bluetooth-action-connect = Connect
bluetooth-action-pair = Pair
bluetooth-device-unnamed = Unnamed device

bluetooth-master-switch = Bluetooth

bluetooth-empty-no-adapter = No Bluetooth adapter found
bluetooth-empty-no-adapter-body = Plug in a Bluetooth adapter to connect devices.
bluetooth-blocked-title = Bluetooth is blocked
bluetooth-blocked-hardware-body = A switch on this computer has turned Bluetooth off. Move it back to turn Bluetooth on.
bluetooth-blocked-software-body = Airplane mode or another system setting has turned Bluetooth off. Turn off airplane mode, then try again.
bluetooth-off-title = Bluetooth is off
bluetooth-off-body = Turn on Bluetooth to find and connect devices.
bluetooth-off-action = Turn on

bluetooth-group-mine = My devices
bluetooth-group-nearby = Nearby devices
bluetooth-scanning = Looking for nearby devices…
bluetooth-empty-title = No devices found
bluetooth-empty-body = Turn on a device and make it visible.

bluetooth-value-yes = Yes
bluetooth-value-no = No
bluetooth-group-details = Details
bluetooth-detail-battery = Battery
bluetooth-detail-type = Type
bluetooth-detail-address = Address
bluetooth-detail-paired = Paired
bluetooth-detail-name = Name
bluetooth-detail-trust = Connects automatically

# $name (String) - the device's name.
bluetooth-forget-title = Forget “{ $name }”?
bluetooth-forget-body = This computer will forget the pairing. Pair again to use the device.
bluetooth-forget-cancel = Cancel

## Connection editor

# Dropdown labels and summaries in the connection editor. Search keywords in settings_index.rs stay literal.
network-editor-metered-automatic = Automatic
network-editor-metered-yes = Yes
network-editor-metered-no = No
network-editor-ipv4-auto = Automatic (DHCP)
network-editor-ipv4-manual = Manual
network-editor-ipv4-link-local = Link-local only
network-editor-ipv4-shared = Shared with other computers
network-editor-ipv4-off = Off
network-editor-ipv6-auto = Automatic
network-editor-ipv6-dhcp = Automatic, DHCP only
network-editor-ipv6-manual = Manual
network-editor-ipv6-link-local = Link-local only
network-editor-ipv6-shared = Shared with other computers
network-editor-ipv6-ignore = Ignore
network-editor-ipv6-off = Off
network-editor-vpn-ipv4-auto = Automatic (DHCP)
network-editor-vpn-ipv4-manual = Manual
network-editor-vpn-ipv4-off = Off
network-editor-vpn-ipv6-auto = Automatic
network-editor-vpn-ipv6-manual = Manual
network-editor-vpn-ipv6-ignore = Ignore
network-editor-vpn-ipv6-off = Off
network-editor-privacy-default = Default
network-editor-privacy-off = Off
network-editor-privacy-public = Prefer public addresses
network-editor-privacy-temporary = Prefer temporary addresses
network-editor-addr-gen-default = Default
network-editor-addr-gen-stable = Stable and private
network-editor-addr-gen-eui64 = From the hardware address
network-editor-multicast-default = Default
network-editor-multicast-off = Off
network-editor-multicast-resolve = Resolve only
network-editor-multicast-on = On
network-editor-dot-default = Default
network-editor-dot-off = Off
network-editor-dot-when-available = When available
network-editor-dot-always = Always
network-editor-retry-default = Default
network-editor-retry-once = Once
network-editor-retry-2 = 2 times
network-editor-retry-3 = 3 times
network-editor-retry-5 = 5 times
network-editor-retry-10 = 10 times
network-editor-retry-forever = Forever
network-editor-band-automatic = Automatic
network-editor-band-2ghz = 2.4 GHz
network-editor-band-5ghz = 5 GHz
network-editor-mac-default = Default
network-editor-mac-never = Never
network-editor-mac-always = Always
network-editor-cloned-default = Default
network-editor-cloned-preserve = Preserve
network-editor-cloned-permanent = Permanent
network-editor-cloned-random = Random
network-editor-cloned-stable = Stable
network-editor-cloned-address = Specific address
network-editor-wol-default = Default
network-editor-wol-off = Off
network-editor-wol-magic = Magic packet
network-editor-wifi-security-none = None
network-editor-wifi-security-wpa-psk = WPA & WPA2 Personal
network-editor-wifi-security-sae = WPA3 Personal
network-editor-wifi-security-wpa-eap = WPA & WPA2 Enterprise
network-editor-wifi-security-wep = WEP
network-editor-eap-peap = PEAP
network-editor-eap-ttls = Tunnelled TLS
network-editor-eap-tls = TLS
network-editor-eap-fast = FAST
network-editor-phase2-mschapv2 = MSCHAPv2
network-editor-phase2-mschap = MSCHAP
network-editor-phase2-pap = PAP
network-editor-phase2-chap = CHAP
network-editor-phase2-gtc = GTC
network-editor-phase2-md5 = MD5
network-editor-dot1x-off = 802.1X off
network-editor-dot1x-on = 802.1X on
network-editor-proxy-off = None
network-editor-proxy-automatic = Automatic

# $address (String) - an IPv4 address set by hand.
network-editor-summary-manual = Manual · { $address }
# $servers (String) - the DNS servers set by hand, joined with commas.
network-editor-dns-automatic-servers = Automatic + { $servers }
network-editor-dns-automatic = Automatic
network-editor-dns-none = None

# Validation hints shown under a field that does not accept what was typed.
network-editor-route-hint = Write a route as network/prefix, next hop and metric, such as 10.0.0.0/8 192.168.1.1 100.
network-editor-route4-hint = Enter an IPv4 route such as 10.0.0.0/8 192.168.1.1.
network-editor-route6-hint = Enter an IPv6 route such as 2001:db8::/32 fe80::1.
network-editor-mtu-hint = Enter a size in bytes from 576 to 65535.
network-editor-metric-hint = Enter -1 for automatic, or a whole number.
network-editor-search-domain-hint = Enter a domain such as example.com.
network-editor-dhcp-hostname-hint = Enter a host name without spaces.
network-editor-file-path-hint = Enter the full path of a file, such as /etc/ssl/certs/ca.pem.
network-editor-wep-key-hint = Use 5 or 13 characters, or 10 or 26 hexadecimal digits.
network-editor-connection-name-empty = Enter a name.

## Network profile

network-profile-advanced-title = Advanced
network-profile-advanced-manual-ipv4 = Manual IPv4
network-profile-advanced-manual-ipv6 = Manual IPv6
network-profile-advanced-ipv6-off = IPv6 off
network-profile-advanced-custom-dns = Custom DNS
network-profile-advanced-proxy = Proxy
network-profile-advanced-802-1x = 802.1X
# $priority (Number) - the connection's autoconnect priority.
network-profile-advanced-priority = Priority { $priority }
network-profile-advanced-only-for-you = Only for you
network-profile-advanced-empty = Addresses, DNS, security and technical details
network-profile-identity-default = Default

## Mobile network

# SIM dialog titles, buttons and confirmations.
# Title of the dialog that unlocks a SIM with its PIN or PUK.
network-mobile-sim-unlock-title = Unlock SIM
# Title of the dialog that turns on the SIM lock.
network-mobile-sim-enable-title = Lock SIM with PIN
# Title of the dialog that turns off the SIM lock.
network-mobile-sim-disable-title = Turn off SIM lock
# Title of the dialog that changes the PIN.
network-mobile-sim-change-title = Change PIN
# Button that submits the unlock dialog.
network-mobile-sim-unlock-action = Unlock
# Button that submits the enable-lock dialog.
network-mobile-sim-enable-action = Lock SIM
# Button that submits the disable-lock dialog.
network-mobile-sim-disable-action = Turn off
# Button that submits the change-PIN dialog.
network-mobile-sim-change-action = Change
# Toast shown after the SIM is unlocked.
network-mobile-sim-unlocked-toast = SIM unlocked
# Toast shown after the SIM lock is turned on.
network-mobile-sim-enabled-toast = SIM lock turned on
# Toast shown after the SIM lock is turned off.
network-mobile-sim-disabled-toast = SIM lock turned off
# Toast shown after the PIN is changed.
network-mobile-sim-changed-toast = PIN changed

# Field labels.
# Label of the PIN field.
network-mobile-sim-pin-label = PIN
# Label of the PUK field, shown when the SIM is blocked.
network-mobile-sim-puk-label = PUK
# Label of the current-PIN field in the change-PIN dialog.
network-mobile-sim-current-pin-label = Current PIN
# Label of the new-PIN field.
network-mobile-sim-new-pin-label = New PIN
# Label of the field that repeats the new PIN.
network-mobile-sim-repeat-pin-label = Repeat new PIN

# Hints shown under the title of each dialog.
# Hint of the unlock dialog.
network-mobile-sim-unlock-hint = Enter the PIN of your SIM.
# Hint of the PUK dialog, shown when the SIM is blocked.
network-mobile-sim-puk-hint = The SIM is blocked after too many wrong PINs. Enter the 8-digit PUK from your carrier, then choose a new PIN.
# Hint of the dialog that turns on the SIM lock.
network-mobile-sim-enable-hint = With the SIM lock on, the PIN is asked for each time the phone starts.
# Hint of the dialog that turns off the SIM lock.
network-mobile-sim-disable-hint = Enter the PIN to stop asking for it when the phone starts.
# Hint of the change-PIN dialog.
network-mobile-sim-change-hint = Choose a PIN of 4 to 8 digits.

# Validation messages.
# Shown when the new PIN and its repeat differ.
network-mobile-sim-pin-mismatch-error = The two PINs are not the same.
# Shown when the new PIN is invalid. $error is the reason.
# $error (String) - why the new PIN is not valid.
network-mobile-sim-new-pin-error = New PIN: { $error }

# Attempts left, shown under the PIN or PUK field.
# Words used for the thing being counted in the attempts note.
network-mobile-sim-pin-word = PIN
network-mobile-sim-puk-word = PUK
# $count (Number) - attempts left. $what (String) - PIN or PUK.
network-mobile-sim-attempts-left = { $count ->
    [0] No { $what } attempts left
    [one] 1 { $what } attempt left
   *[other] { $count } { $what } attempts left
}

## Mobile APN

# Authentication method labels, in the order shown in the picker.
network-apn-auth-auto = Automatic
network-apn-auth-none = None
network-apn-auth-pap = PAP
network-apn-auth-chap = CHAP
network-apn-auth-pap-chap = PAP or CHAP
network-apn-auth-mschap = MSCHAP
network-apn-auth-mschapv2 = MSCHAPv2
network-apn-auth-eap = EAP
network-apn-auth-custom = Custom

# IP type labels.
network-apn-ip-v4 = IPv4
network-apn-ip-v6 = IPv6
network-apn-ip-v4v6 = IPv4 and IPv6

# Row summaries for an access point without a typed name.
network-apn-summary-detected = Detected from the carrier
network-apn-summary-empty = No access point name

# Validation messages.
network-apn-error-length = Use 64 characters or fewer.
network-apn-error-characters = Use only letters, numbers, dots, hyphens and underscores.
network-apn-error-name-taken = Use a name that no other connection has.

# Error and toast messages.
network-apn-error-read = Could not read the access point names. Try again.
network-apn-error-read-one = Could not read the access point. Try again.
network-apn-error-switch = Could not switch the access point. Try again.
network-apn-error-save = Could not save the access point. Try again.
network-apn-error-delete = Could not delete the access point. Try again.
network-apn-retry = Try again

# The list of access points.
network-apn-empty = No access point names yet.
network-apn-edit-tooltip = Edit access point
network-apn-list-title = Access point names
network-apn-add = Add access point…

# The editor form.
network-apn-name-label = Name
network-apn-name-placeholder = Carrier
network-apn-detect-switch = Detect settings automatically
network-apn-section-title = Access point
network-apn-apn-label = APN
network-apn-apn-hint = Leave empty for none.
network-apn-user-label = User name
network-apn-password-label = Password
network-apn-auth-label = Authentication
network-apn-ip-label = IP type
network-apn-delete = Delete access point
network-apn-add-button = Add access point

# The delete dialog. $name is the access point name.
# $name (String) - the name of the access point being deleted.
network-apn-delete-title = Delete “{ $name }”?
network-apn-delete-body = Mobile data stops working if no other access point is left.
network-apn-cancel = Cancel
network-apn-delete-confirm = Delete

## Mobile carrier

# Availability words shown under a carrier found by a search.
network-carrier-availability-current = Connected
network-carrier-availability-forbidden = Not allowed by your SIM
network-carrier-availability-available = Available

# Errors and toasts.
network-carrier-error-read = Could not read the modem. Try again.
network-carrier-error-search = Could not search for networks. Try again.
network-carrier-error-automatic = Could not switch to automatic network selection. Try again.
network-carrier-error-join = Could not join that network. Try again.
network-carrier-retry = Try again

# States of the page.
network-carrier-modem-empty = No mobile modem found
network-carrier-locked = Unlock the SIM to choose a carrier.
network-carrier-no-sim = No SIM card
network-carrier-off = Mobile network is off

# The page itself.
network-carrier-automatic = Automatic
network-carrier-section-title = Networks
network-carrier-idle = Search to see the carriers around you.
network-carrier-searching = Searching for networks…
network-carrier-empty = No networks found
network-carrier-search-again = Search again

## Mobile page

# One-line status under the Mobile network row. Shown when Settings cannot tell more.
# Status when airplane mode is on.
network-mobile-status-airplane = Off in airplane mode
# Status when the mobile radio is off.
network-mobile-status-off = Off
# Status when the link has no connection.
network-mobile-status-not-connected = Not connected
# Status when the SIM is locked with a PIN or PUK.
network-mobile-status-sim-locked = SIM locked
# Status when no SIM card is inserted.
network-mobile-status-no-sim = No SIM card
# Status while the modem is searching for a network.
network-mobile-status-searching = Searching for a network
# Status when the network refused the registration.
network-mobile-status-refused = Registration refused
# Status when the modem is not registered on any network.
network-mobile-status-not-registered = Not registered
# Fallback network name when the carrier is not known.
network-mobile-status-network = Mobile network
# Appended to the status when the modem is roaming.
network-mobile-status-roaming = Roaming
# Error when the modem does not become ready in time.
network-mobile-modem-not-ready = The modem did not become ready. Try again in a moment.
# Error when mobile data cannot be turned on.
network-mobile-data-on-error = Could not turn on mobile data. Try again.
# Error when mobile data cannot be turned off.
network-mobile-data-off-error = Could not turn off mobile data. Try again.
# Row that opens the mobile network page.
network-mobile-row = Mobile network
# Page subtitle while the modem is loading.
network-mobile-loading = Loading…
# Info shown when airplane mode is on.
network-mobile-airplane-info = Mobile network is off in airplane mode.
# Button that turns airplane mode off.
network-mobile-airplane-off = Turn off airplane mode
# Hint shown when there is no SIM card.
network-mobile-no-sim-hint = No SIM card. Insert one to use mobile data.
# Hint shown when the mobile network is switched off.
network-mobile-off-hint = The mobile network is off. Turn on mobile data to use it.
# Error title shown when the network status cannot be read.
network-mobile-status-read-error = Could not read the network status. Try again in a moment.
# Button that retries a failed load.
network-mobile-retry = Try again
# Empty state when no mobile modem is found.
network-mobile-modem-empty = No mobile modem found
# Error title shown when the modem cannot be read.
network-mobile-modem-read-error = Could not read the modem. Try again.
# Row that opens the advanced mobile settings.
network-mobile-advanced = Advanced
# Row showing the carrier name.
network-mobile-carrier = Carrier
# Row showing the network technology, such as 4G or 5G.
network-mobile-network-type = Network type
# Row showing the signal strength.
network-mobile-signal = Signal
# Group title for registration details.
network-mobile-registration-title = Registration
# Row showing the registration state.
network-mobile-registration = Registration
# Row showing whether the modem is roaming.
network-mobile-roaming = Roaming
# Value when roaming is on.
network-mobile-yes = Yes
# Value when roaming is off.
network-mobile-no = No
# Switch that turns mobile data on or off.
network-mobile-data = Mobile data
# Switch that allows data roaming.
network-mobile-data-roaming = Data roaming
# Row choosing the preferred network type.
network-mobile-preferred-type = Preferred network type
# Value when the carrier is chosen by hand.
network-mobile-manual = Manual
# Value when the carrier or APN is chosen automatically.
network-mobile-automatic = Automatic
# Value when no APN is set.
network-mobile-none = None
# Group title for the network and APN rows.
network-mobile-network-title = Network
# Row that opens the carrier choice.
network-mobile-network = Network
# Row that opens the access point names.
network-mobile-apn-row = Access point names
# Group title for the SIM card.
network-mobile-sim-title = SIM
# Row showing the SIM operator.
network-mobile-sim-operator = Operator
# Row showing the SIM number.
network-mobile-sim-number = SIM number
# Switch that locks the SIM with a PIN.
network-mobile-sim-lock = Lock SIM with PIN
# Row that opens the PIN change dialog.
network-mobile-sim-change-pin = Change PIN…
# Info when the SIM needs a code that Settings cannot enter.
network-mobile-sim-other-lock = This SIM needs a code that Settings cannot enter.
# Info when the SIM is locked with an unknown lock.
network-mobile-sim-locked = The SIM is locked.
# Info when the SIM is blocked after too many wrong PINs.
network-mobile-sim-puk-blocked = The SIM is blocked after too many wrong PINs.
# Row that opens the PUK unlock dialog.
network-mobile-sim-unlock-puk = Unlock with PUK
# Info when the SIM is locked with a PIN.
network-mobile-sim-pin-locked = The SIM is locked with a PIN.
# Row that opens the PIN unlock dialog.
network-mobile-sim-unlock-pin = Unlock with PIN
# Group title for the modem details.
network-mobile-modem-title = Modem
# Modem detail: manufacturer.
network-mobile-modem-manufacturer = Manufacturer
# Modem detail: model.
network-mobile-modem-model = Model
# Modem detail: firmware revision.
network-mobile-modem-firmware = Firmware
# Modem detail: IMEI number.
# IMEI is an international identifier, not a word to translate.
network-mobile-modem-imei = IMEI
# Modem detail: phone numbers.
network-mobile-modem-phone-number = Phone number
# Modem detail: primary port.
network-mobile-modem-port = Port

## Proxy page

# Proxy mode: none.
network-proxy-mode-none = None
# Proxy mode: manual.
network-proxy-mode-manual = Manual
# Proxy mode: automatic.
network-proxy-mode-auto = Automatic
# Row title on the Network page.
network-proxy-row = Proxy
# Status while proxy settings are read.
network-proxy-reading = Reading proxy settings…
# Error when proxy settings cannot be read.
network-proxy-read-failed = Could not read the proxy settings. Try again.
# Retry button for a failed read.
network-proxy-retry = Try again
# Error when proxy settings cannot be saved.
network-proxy-save-failed = Could not save the proxy settings. Try again.
# Label for the proxy method choice.
network-proxy-method = Method
# Group title for automatic configuration.
network-proxy-auto-group = Automatic configuration
# Label for the configuration URL field.
network-proxy-config-url-label = Configuration URL
# Hint under the configuration URL field.
network-proxy-config-hint = Leave empty to find the configuration automatically.
# Group title for the HTTP proxy.
network-proxy-http-group = HTTP proxy
# Switch that applies the HTTP proxy to every protocol.
network-proxy-same-protocols = Use this proxy for every protocol
# Group title for the HTTPS proxy.
network-proxy-https-group = HTTPS proxy
# Group title for the FTP proxy.
network-proxy-ftp-group = FTP proxy
# Group title for the SOCKS host.
network-proxy-socks-group = SOCKS host
# Label for a proxy host field.
network-proxy-host-label = Host
# Label for a proxy port field.
network-proxy-port-label = Port
# Group title for the ignore list.
network-proxy-ignore-group = Ignore hosts
# Label for the ignore list field.
network-proxy-hosts-label = Hosts
# Hint under the ignore list.
network-proxy-ignore-hint = Addresses that skip the proxy. Separate them with commas. Write a network as 192.168.0.0/16.
# Error for a port outside 1 to 65535.
network-proxy-port-range-error = Enter a port number from 1 to 65535.
# Error when an ignore entry contains a space.
network-proxy-spaces-error = Separate hosts with commas. Hosts cannot contain spaces.

## Mobile modem

# Friendly error: the PIN is wrong.
network-mobile-friendly-wrong-pin = That PIN is not right.
# Friendly error: the SIM needs its PUK.
network-mobile-friendly-puk = The SIM is blocked and needs its PUK code.
# Friendly error: the SIM needs its PIN first.
network-mobile-friendly-pin-first = The SIM needs its PIN first.
# Error when the modem does not answer.
network-mobile-modem-no-response = The modem did not respond. Try again.
# Error when the modem cannot be read.
network-mobile-modem-unreadable = Could not read the modem. Try again.
# Error when the SIM cannot be read.
network-mobile-sim-unreadable = Could not read the SIM. Try again.
# Error when no modem is found.
network-mobile-no-modem-found = No mobile modem found. Check that it is connected.
# Error for a PIN outside 4 to 8 digits.
network-mobile-pin-range-error = Enter 4 to 8 digits.
# Error for a PUK that is not 8 digits.
network-mobile-puk-range-error = Enter the 8-digit PUK.
# Signal strength: none.
network-mobile-signal-none = No signal
# Signal strength: weak.
network-mobile-signal-weak = Weak
# Signal strength: fair.
network-mobile-signal-fair = Fair
# Signal strength: good.
network-mobile-signal-good = Good
# Signal strength: excellent.
network-mobile-signal-excellent = Excellent
# Registration: home network.
network-mobile-reg-home = Home network
# Registration: roaming.
network-mobile-reg-roaming = Roaming
# Registration: searching.
network-mobile-reg-searching = Searching
# Registration: refused.
network-mobile-reg-refused = Refused
# Registration: not registered.
network-mobile-reg-idle = Not registered
# Registration: texts only.
network-mobile-reg-texts-only = Texts only
# Registration: emergency calls only.
network-mobile-reg-emergency-only = Emergency only
# Registration: unknown state.
network-mobile-reg-unknown = Unknown
# Registration: registered.
network-mobile-reg-registered = Registered
# Network mode: automatic.
network-mobile-mode-automatic = Automatic
# Network mode: one generation only. The generation is 2G, 3G, 4G or 5G.
network-mobile-mode-only = { $generation } only

## Profile proxy

# Row title for the proxy method on a connection.
network-profile-proxy-method = Proxy
# Switch that limits the proxy to web browsers.
network-profile-proxy-browser-only = Only for web browsers
# Group title for the proxy on a connection.
network-profile-proxy-group = Proxy
# Group title for the automatic configuration.
network-profile-proxy-auto-group = Automatic configuration
# Label for the configuration address field.
network-profile-proxy-address-label = Address
# Hint under the configuration address field.
network-profile-proxy-address-hint = Where the proxy configuration file is.
# Label for the configuration script field.
network-profile-proxy-script-label = Script
# Hint under the configuration script field.
network-profile-proxy-script-hint = The text of a proxy configuration file.

## VPN tunnel

# Toast after saving when a running tunnel needs reconnecting.
network-vpn-reconnect-note = Saved. Turn the VPN off and on to apply the changes.
# Button that retries loading a VPN profile.
network-vpn-try-again = Try again
# Group title for the tunnel summary while a profile loads.
network-vpn-group-tunnel = Tunnel
# Row label for a VPN server.
network-vpn-row-server = Server
# Row label for the sign-in method of a VPN.
network-vpn-row-sign-in = Sign in with
# Error when a DNS server has no address of the given family on the tunnel.
network-vpn-dns-needs-ipv4 = A DNS server needs an IPv4 address for the tunnel. Add one above or remove the server.
# Error when a DNS server has no address of the given family on the tunnel.
network-vpn-dns-needs-ipv6 = A DNS server needs an IPv6 address for the tunnel. Add one above or remove the server.
# Note under the button that generates a new WireGuard key.
network-vpn-new-key-note = A new key needs the other side to be given the new public key.
# Button that generates a new WireGuard private key.
network-vpn-generate-key = Generate new key
# Field label for the tunnel's own addresses.
network-vpn-field-addresses = Addresses
# Hint under the tunnel addresses field.
network-vpn-hint-addresses = This computer's addresses inside the tunnel, one per line, with the prefix length.
# Field label for the tunnel's DNS servers.
network-vpn-field-dns = DNS servers
# Hint under the DNS servers field.
network-vpn-hint-dns = Used while the tunnel is up, one per line.
# Field label for the WireGuard listen port.
network-vpn-field-listen-port = Listen port
# Placeholder for an optional value that is chosen automatically.
network-vpn-placeholder-automatic = Automatic
# Unit after an MTU or size value.
network-vpn-unit-bytes = bytes
# Field label for the firewall mark.
network-vpn-field-firewall-mark = Firewall mark
# Placeholder for an optional value that is not set.
network-vpn-placeholder-none = None
# Group title for the WireGuard peers.
network-vpn-group-peers = Peers
# Title of one WireGuard peer; $number is its position from 1.
network-vpn-peer-title = Peer { $number }
# Button and row that add a new WireGuard peer.
network-vpn-add-peer = Add peer
# Subtitle of a peer with no endpoint.
network-vpn-no-endpoint = No endpoint
# Subtitle of a peer that routes all traffic.
network-vpn-all-traffic = all traffic
# Subtitle of a peer with several allowed address ranges; $count is the number of ranges.
network-vpn-address-ranges = { $count ->
    [one] 1 address range
   *[other] { $count } address ranges
}
# Message on a plugin VPN where peers do not apply.
network-vpn-only-wireguard-peers = Only a WireGuard VPN has peers.
# Group title for a single peer's settings.
network-vpn-group-peer = Peer
# Row label for a key.
network-vpn-row-public-key = Public key
# Row label for a peer endpoint.
network-vpn-row-endpoint = Endpoint
# Error when two peers in one tunnel share a public key.
network-vpn-duplicate-key = Another peer of this tunnel already has this public key.
# Group title for the traffic routing of a peer.
network-vpn-group-traffic = Traffic
# Switch that sends all traffic through the tunnel.
network-vpn-route-all = Route all traffic
# Field label for the networks a peer carries.
network-vpn-field-allowed = Allowed addresses
# Hint under the allowed addresses field.
network-vpn-hint-allowed = Networks that go through this peer, one per line, such as 10.8.0.0/24.
# Hint under the peer public key field.
network-vpn-hint-public-key = The other side's public key.
# Field label for the preshared key.
network-vpn-field-preshared = Preshared key
# Hint under the preshared key field.
network-vpn-hint-preshared = An extra secret both sides share, if they use one.
# Hint under the peer endpoint field.
network-vpn-hint-endpoint = Where to reach it, as host:port. Leave empty if it connects to you.
# Field label for the keepalive interval.
network-vpn-field-keepalive = Keepalive
# Placeholder for an optional value that is switched off.
network-vpn-placeholder-off = Off
# Unit after a keepalive value.
network-vpn-unit-seconds = seconds
# Hint under the keepalive field.
network-vpn-hint-keepalive = Keeps the connection open through a router. 25 is usual.
# Row that deletes a peer.
network-vpn-delete-peer = Delete peer
# Confirmation title for deleting a peer; $label is the peer's title.
network-vpn-delete-peer-title = Delete { $label }?
# Confirmation body for deleting a peer.
network-vpn-delete-peer-body = The tunnel to this peer stops.
# Button that cancels a dialog.
network-vpn-cancel = Cancel
# Button that confirms a destructive delete.
network-vpn-delete = Delete
# Error when a required plugin field is empty; $label is the field's name.
network-vpn-field-needed = { $label } is needed.
# Choice for a remembered setting that is not needed.
network-vpn-not-needed = Not needed
# Choice label for remembering a plugin field; $label is the field's name in lowercase.
network-vpn-remember = Remember { $label }
# Group title for a plugin's extra settings.
network-vpn-group-advanced = Advanced
# Field label for a new plugin setting's name.
network-vpn-field-new-setting = New setting
# Placeholder for a new plugin setting's name.
network-vpn-placeholder-name = name
# Field label for a new plugin setting's value.
network-vpn-field-value = Value
# Button that adds a plugin setting.
network-vpn-add-setting = Add setting
# Error when a plugin setting's name contains = or a comma.
network-vpn-setting-no-equals = A setting's name cannot contain = or a comma.
# Error when a plugin setting name is already listed.
network-vpn-setting-taken = This setting is already in the list.
# Tooltip on the button that removes a plugin setting.
network-vpn-remove-setting = Remove setting
# Field label for the tunnel's MTU.
network-vpn-field-mtu = MTU
# Field label for the WireGuard private key.
network-vpn-field-private-key = Private key
# Hint under the private key field.
network-vpn-hint-private-key = Stays on this computer. Never share it.
# Group title for the interface section of a VPN profile.
network-vpn-group-interface = Interface
# Value shown when a public key is not available yet.
network-vpn-not-available = Not available

## Wi-Fi hotspot
# Error when the hotspot settings cannot be read.
network-hotspot-error-read = Could not read the hotspot settings. Try again.
# Button to retry a failed load.
network-hotspot-try-again = Try again
# Action to share the connection through the hotspot.
network-hotspot-share = Share
# Action to turn the hotspot off.
network-hotspot-turn-off = Turn off
# Status shown while the hotspot is turning off.
network-hotspot-turning-off = Turning off { $name }…
# Error when the hotspot cannot be turned off.
network-hotspot-error-stop = Could not turn off the hotspot. Try again.
# Status when the computer has no Wi-Fi adapter.
network-hotspot-no-adapter = This computer has no Wi-Fi adapter.
# Status when a hardware switch has turned Wi-Fi off.
network-hotspot-hardware-switch = A hardware switch has turned Wi-Fi off.
# Status when airplane mode blocks the hotspot.
network-hotspot-airplane = Airplane mode has turned Wi-Fi off, so there is no connection to share.
# Action to turn off airplane mode.
network-hotspot-turn-off-airplane = Turn off airplane mode
# Status when Wi-Fi is off.
network-hotspot-wifi-off = Wi-Fi is off.
# Action to turn Wi-Fi on.
network-hotspot-turn-on-wifi = Turn on Wi-Fi
# Band option meaning the hotspot picks a band itself.
network-hotspot-band-auto = Automatic
# Status shown while the hotspot is starting.
network-hotspot-turning-on = Turning on the Wi-Fi hotspot…
# Error when the hotspot cannot be started.
network-hotspot-error-start = Could not turn on the hotspot. Try again.
# Label for the hotspot network name field.
network-hotspot-field-name = Network name
# Label for the hotspot password field.
network-hotspot-field-password = Password
# Hint under the hotspot password field.
network-hotspot-hint-password = At least 8 characters.
# Label for the hotspot band field.
network-hotspot-field-band = Band
# Warning that turning on the hotspot disconnects from the current network.
network-hotspot-disconnects = Turning it on disconnects this computer from { $network }.
# Description of the hotspot sharing behaviour.
network-hotspot-shares = Devices that join share this computer's connection.
# Action to turn the hotspot on.
network-hotspot-turn-on = Turn on
# Value shown when no hotspot password is saved.
network-hotspot-password-none = None saved
# Error value when the hotspot password cannot be read.
network-hotspot-password-error = Could not read the password
# Value shown when no devices are connected.
network-hotspot-devices-none = No devices
# Value shown for one connected device.
network-hotspot-devices-one = 1 device
# Value shown for several connected devices.
network-hotspot-devices-many = { $count ->
    [one] 1 device
   *[other] { $count } devices
}
# Title of the group shown while the hotspot is on.
network-hotspot-group-on = Hotspot is on
# Row label for the connected devices count.
network-hotspot-row-connected = Connected devices

## Connection detail
# Route description with no next hop.
network-route-direct = { $destination } direct
# Route description via a next hop.
network-route-via = { $destination } via { $hop }
# Route description with a metric appended.
network-route-metric = { $route }, metric { $metric }
# IP method: automatic.
network-method-auto = Automatic
# IP method: automatic via DHCP.
network-method-dhcp = Automatic (DHCP)
# IP method: manual.
network-method-manual = Manual
# IP method: off.
network-method-off = Off
# IP method: link-local only.
network-method-link-local = Link-local only
# IP method: shared with other computers.
network-method-shared = Shared with other computers
# Connection type: Ethernet.
network-kind-ethernet = Ethernet
# Connection type: Wi-Fi.
network-kind-wifi = Wi-Fi
# Connection type: mobile broadband.
network-kind-mobile = Mobile broadband
# Connection type: generic VPN.
network-kind-vpn = VPN
# Connection type: named VPN service.
network-kind-vpn-named = VPN ({ $name })
# Connection type: bridge.
network-kind-bridge = Bridge
# Security value for an open Wi-Fi network.
network-security-open = Open

## Profile advanced
# Validation error for a client ID.
network-advanced-client-id-invalid = Use up to 255 characters, with no space at either end.
# Validation error for a route metric.
network-advanced-route-metric-invalid = Enter a whole number, or leave it empty for automatic.
# Section title for connection options.
network-advanced-group-connection = Connection
# Row: retry count when connecting.
network-advanced-retries = Connection retries
# Row: multicast DNS setting.
network-advanced-mdns = Multicast DNS
# Row: LLMNR setting.
network-advanced-llmnr = LLMNR
# Row: hidden network setting.
network-advanced-hidden = Hidden network
# Row: Wi-Fi band.
network-advanced-band = Frequency band
# Row: require IPv4 to connect.
network-advanced-ipv4-required = Require IPv4 to connect
# Row: send host name to DHCP server.
network-advanced-send-hostname = Send host name
# Field label: DHCP host name.
network-advanced-hostname = Host name
# Hint under the host name field.
network-advanced-hostname-hint = Leave empty to send this computer's own name.
# Field label: DHCP client ID.
network-advanced-client-id = Client ID
# Hint under the client ID field.
network-advanced-client-id-hint = mac, duid, or a hex string
# Field label: route metric.
network-advanced-route-metric = Route metric
# Placeholder for an automatic value.
network-advanced-automatic = Automatic
# Hint under the route metric field.
network-advanced-metric-hint = Lower numbers are preferred.
# Row: require IPv6 to connect.
network-advanced-ipv6-required = Require IPv6 to connect
# Row: IPv6 address generation mode.
network-advanced-addr-gen = Address generation

## Profile identity
# Note on a VPN page where hardware settings do not apply.
network-identity-vpn-no-hardware = A VPN has no hardware address or MTU of its own.
# Section title for identity.
network-identity-group = Identity
# Row: connection name.
network-identity-name = Name
# Row: cloned MAC address mode.
network-identity-cloned-mac = Cloned MAC address
# Row: MTU mode.
network-identity-mtu = MTU
# Field label: MAC address.
network-identity-mac-address = MAC address
# Field label: MTU size.
network-identity-mtu-size = MTU size
# Section title for hardware.
network-identity-group-hardware = Hardware
# Row: random MAC address mode.
network-identity-random-mac = Random MAC address
# Row: wake on LAN setting.
network-identity-wake-on-lan = Wake on LAN

## Profile security
# Validation error for a WEP key phrase.
network-security-wep-phrase-error = Use 1 to 64 letters, numbers or symbols.
# Button: open the file chooser.
network-security-browse = Browse…
# File filter: every file.
network-security-all-files = All files
# File filter name: certificates.
network-security-cert-filter = Certificates
# File filter name: private keys.
network-security-key-filter = Private keys
# Row: EAP method.
network-security-method = Method
# Section title for 802.1X sign-in.
network-security-auth-group = Authentication
# Row: inner (phase 2) authentication.
network-security-inner-auth = Inner authentication
# Field label: 802.1X identity.
network-security-identity = Identity
# Field label: anonymous identity.
network-security-anon-identity = Anonymous identity
# Hint under the anonymous identity field.
network-security-anon-hint = Sent before the secure connection starts. Leave empty unless the network asks for it.
# Field label: password.
network-security-password = Password
# Row: use the system's trusted certificates.
network-security-system-ca = Use the system's trusted certificates
# Section title for certificates.
network-security-certs-group = Certificates
# Field label: CA certificate.
network-security-ca-cert = CA certificate
# Hint under the CA certificate field.
network-security-ca-hint = Optional. The certificate that signed the network's own. Leave empty to skip this check.
# Field label: client certificate.
network-security-client-cert = Client certificate
# Field label: private key.
network-security-private-key = Private key
# Field label: private key password.
network-security-key-password = Private key password
# Note on a VPN security page.
network-security-vpn-none = This connection has no security settings here.
# Row: 802.1X switch.
network-security-8021x = Use 802.1X authentication
# Section title and row label: security.
network-security-label = Security
# Description: open network.
network-security-open-note = Open network: traffic is not encrypted.
# Description: WEP network.
network-security-wep-note = WEP is easy to break.
# Hint under the Wi-Fi password field.
network-security-psk-hint = Use 8 to 63 characters, or 64 hexadecimal digits.
# Row: WEP key type.
network-security-key-type = Key type
# Field label: WEP key.
network-security-wep-key = WEP key

## Network
# Title of the Advanced section of a connection page.
network-section-advanced = Advanced
# Title of the IPv4 section of a connection page.
network-section-ipv4 = IPv4
# Title of the IPv6 section of a connection page.
network-section-ipv6 = IPv6
# Title of the DNS section of a connection page.
network-section-dns = DNS
# Title of the Security section of a connection page.
network-section-security = Security
# Title of the Identity section of a connection page.
network-section-identity = Identity
# Title of the Proxy section of a connection page.
network-section-proxy = Proxy
# Title of the other options section of a connection page.
network-section-options = Other options
# Title of the data usage section of a connection page.
network-section-usage = Data usage
# Title of the Tunnel section of a VPN page.
network-section-tunnel = Tunnel
# Title of a VPN peer section. $number (Number) - the peer's position, starting at 1.
network-section-peer = Peer { $number }
# One-line status under the Network page title while airplane mode is on.
network-summary-airplane = Offline (airplane mode)
# One-line status when the network requires a sign-in.
network-summary-sign-in = Sign-in required
# One-line status over Wi-Fi. $connection (String) - the Wi-Fi network name.
network-summary-wifi = Online through Wi-Fi ({ $connection })
# One-line status over the mobile network.
network-summary-modem = Online through the mobile network
# One-line status over a cable.
network-summary-ethernet = Online through Ethernet
# One-line status when connected to a network without internet.
network-summary-no-internet = Connected, no internet
# One-line status when offline.
network-summary-offline = Offline
# Subtitle while the Network page loads.
network-loading = Loading…
# Error when the sign-in page cannot be opened.
network-portal-open-failed = Could not open the sign-in page. Try again.
# Status value when the connection is limited.
network-status-limited = Limited
# Status value when there is no internet.
network-status-no-internet = No internet
# Subtitle of a connected network with no address yet.
network-status-connected = Connected
# Subtitle of a connected network. $address (String) - the IP address.
network-status-connected-address = Connected · { $address }
# Error when the network status cannot be read.
network-status-read-failed = Could not read the network status. Try again in a moment.
# Button to retry a failed network load.
network-try-again = Try again
# Row label for the airplane mode switch.
network-airplane-mode = Airplane mode
# Row label when airplane mode is on.
network-airplane-on = Airplane mode is on
# Row hint when airplane mode is on.
network-airplane-on-hint = Turn it off below to connect.
# Row label when nothing is connected.
network-not-connected = Not connected
# Row hint when nothing is connected.
network-not-connected-hint = Choose a Wi-Fi network or plug in a cable.
# Title of the Connections group on the Network page.
network-group-connections = Connections
# Status when the computer has no network devices.
network-connections-empty = No network devices
# Title of the More group on the Network page.
network-group-more = More
# Error when the network settings cannot be read.
network-read-failed = Could not read the network settings. Try again.
# Title of a wired device with no profile. $device (String) - the interface name, such as enp42s0.
network-ethernet-title-unnamed = Ethernet ({ $device })
# Title of a wired device whose profile name another device shares. $connection (String) - the profile name. $device (String) - the interface name.
network-ethernet-title-shared = { $connection } ({ $device })
# Link line with a speed. $word (String) - the link state, such as Connected. $speed (String) - the link speed, such as 1 Gbit/s.
network-link-with-speed = { $word } · { $speed }
# Link state while a connection is being made.
network-link-connecting = Connecting
# Link state while a connection is being closed.
network-link-disconnecting = Disconnecting
# Link state when no cable is plugged in.
network-link-cable-unplugged = Cable unplugged
# Link state when the device is connected.
network-link-connected = Connected
# Link state when the device is not connected.
network-link-not-connected = Not connected
# Link line with an address. $state (String) - the link state. $address (String) - the IP address.
network-link-with-address = { $state } · { $address }
# Error when a wired connection cannot be made.
network-ethernet-error-connect = Could not connect. Check the cable and try again.
# Error when a wired connection cannot be closed.
network-ethernet-error-disconnect = Could not disconnect. Try again.
# Row title of one spare wired adapter in the Connections group.
network-ethernet-other-one = 1 other wired adapter
# Row title of spare wired adapters in the Connections group. $count (Number) - how many adapters.
network-ethernet-other-many = { $count } other wired adapters
# Right-click menu item to disconnect a wired device.
network-ethernet-menu-disconnect = Disconnect
# Right-click menu item to connect a wired device.
network-ethernet-menu-connect = Connect
# Right-click menu item to open a wired device's settings.
network-ethernet-menu-settings = Settings
# Error when a wired connection change fails.
network-ethernet-error-change = Could not change the connection. Try again.
# Title of the dialog that confirms deleting a profile. $name (String) - the profile name.
network-ethernet-delete-title = Delete “{ $name }”?
# Body of the dialog that confirms deleting a profile.
network-ethernet-delete-body = This removes the profile and its saved settings.
# Button to cancel a dialog.
network-cancel = Cancel
# Button to delete a profile.
network-delete = Delete
# Label of the IP address fact on a connection page.
network-facts-ip-address = IP address
# Label of the speed fact on a connection page.
network-facts-speed = Speed
# Strip button to disconnect a wired device.
network-ethernet-disconnect = Disconnect
# Strip button to connect a wired device.
network-ethernet-connect = Connect
# Label of the profile choice on a wired device's page.
network-ethernet-profile = Profile
# Row that opens the list of wired profiles.
network-ethernet-manage-profiles = Manage profiles
# Button and menu item to use a wired profile now.
network-ethernet-use-profile = Use this profile
# Menu item to open a wired profile's page.
network-ethernet-open = Open
# Row button to use a wired profile.
network-ethernet-use = Use
# Subtitle of the wired profile that is running.
network-ethernet-in-use = In use
# Error shown when wired profiles cannot be read.
network-ethernet-profiles-error = Could not read the connection profiles. Try again.
# Row to add a wired profile.
network-ethernet-add-profile = Add profile…
# Title of the popup that names a new profile.
network-profile-new-title = New profile
# Field error when another profile already has the name.
network-profile-name-taken = Use a name that no other profile has.
# Field error when the profile name is empty.
network-profile-name-empty = Enter a name.
# Label of the profile name field.
network-profile-name-label = Name
# Placeholder of the profile name field, an example name.
network-profile-name-placeholder = Office
# Button that creates a new profile.
network-add = Add
# Subtitle of a wired profile that is not running.
network-ethernet-not-in-use = Not in use
# Title of the Profiles group on a wired device's Advanced page.
network-ethernet-profiles-title = Profiles
# Fact row labels on the technical details page.
network-facts-method = Method
network-facts-address = Address
network-facts-gateway = Gateway
network-facts-dns-server = DNS server
network-facts-search-domain = Search domain
# Row label for one route; the destination is the connection's route target.
# { $destination }: the route destination, such as 192.168.0.0/24.
network-facts-route-to = Route to { $destination }
network-facts-technical-details = Technical details
# Notice on the technical details page when the connection is not active.
network-facts-not-active = This connection is not active. These are its saved settings, not live values.
network-facts-device = Device
network-facts-type = Type
network-facts-uuid = UUID
network-facts-mac = MAC address
network-facts-mtu = MTU
network-facts-access-point = Access point
network-facts-band = Band
network-facts-channel = Channel
network-facts-security = Security
network-facts-error-details = Could not read the connection details. Try again.
network-facts-try-again = Try again
network-facts-reading = Reading connection…
network-facts-error-connection = Could not read the connection. Try again.
# Error when the airplane mode switch fails to turn on.
network-airplane-on-failed = Could not turn on airplane mode. Try again.
# Error when the airplane mode switch fails to turn off.
network-airplane-off-failed = Could not turn off airplane mode. Try again.
# Error when the Wi-Fi radio fails to turn on.
network-wifi-on-failed = Could not turn on Wi-Fi. Try again.
# Error when the Wi-Fi radio fails to turn off.
network-wifi-off-failed = Could not turn off Wi-Fi. Try again.
# Error when the network status cannot be read.
network-status-error-read = Could not read the network status. Try again in a moment.
network-dns-both-off = IPv4 and IPv6 are both off for this connection. Turn one on to set DNS servers.
network-dns-automatic = Automatic DNS
network-dns-over-tls = DNS over TLS
network-dns-server = Server
network-dns-domain = Domain
network-dns-add-server = Add server
network-dns-add-search = Add search domain
network-dns-section = DNS
network-dns-servers-section = Servers
network-dns-search-section = Search domains
network-ip-method = Method
network-ip-address = Address
network-ip-add-address = Add address
network-ip-addresses = Addresses
network-ip-gateway = Gateway
network-ip-route = Route
network-ip-add-route = Add route
network-ip-static-routes = Static routes
network-ip-auto-routes = Use automatic routes
network-ip-never-default = Use only for resources on this network
network-ip-privacy = Privacy extensions
network-profile-autoconnect = Connect automatically
network-profile-metered = Metered connection
network-profile-password = Password
network-profile-not-saved = Not saved
network-profile-nothing-here = Nothing to set here.
network-profile-addressing = Addressing
network-profile-proxy-automatic = Automatic
network-profile-proxy-none = None
network-profile-priority = Priority
network-profile-other-users = Available to other users
network-profile-saved-reconnect = Saved. Reconnect to apply the change.
network-profile-reconnect = Reconnect
network-profile-error-reconnect = Could not reconnect. Try again.
network-profile-error-save-change = Could not save the change. Try again.
network-profile-error-save-settings = Could not save the settings. Try again.
network-identity-mtu-automatic = Automatic
network-identity-mtu-custom = Custom
network-identity-bssid = BSSID
network-identity-bytes = bytes
network-security-wep-passphrase = Passphrase
network-security-ca-certificate = CA certificate
network-security-client-certificate = Client certificate
network-security-wep-type-key = Key

## Data usage
# Window and page titles for the data usage screen.
data-usage-title = Data usage
data-usage-col-type = Type
data-usage-received = Received
data-usage-sent = Sent
data-usage-total = Total
data-usage-totals = Totals
data-usage-period = Period
data-usage-show = Show
data-usage-from = From
data-usage-to = To
data-usage-try-again = Try again
data-usage-about = About this data
data-usage-source = Source
data-usage-source-own = Recorded by Bingux
data-usage-nothing-recorded-yet = Nothing recorded yet
data-usage-no-data = No data yet
data-usage-near-limit = Near limit
data-usage-over-limit = Over limit
data-usage-by-interface = By interface
data-usage-interface = Interface
data-usage-no-interface = No interface has been recorded yet.
data-usage-by-connection = By connection
data-usage-connection = Connection
data-usage-no-connection = No connection has been recorded yet.
data-usage-kind-wifi = Wi-Fi
data-usage-kind-ethernet = Ethernet
data-usage-kind-mobile = Mobile
data-usage-kind-vpn = VPN
data-usage-chart-today = Today by hour
data-usage-chart-week = This week by day
data-usage-chart-month = This month by day
data-usage-chart-cycle = This billing cycle by day
data-usage-chart-custom = Chosen dates by day
# $period is the chart title in lower case, for example "this week by day".
# $received and $sent are sizes such as "1.2 GB".
data-usage-chart-label = Data received and sent, { $period }. Received { $received }, sent { $sent }.
data-usage-nothing-recorded = Nothing has been recorded yet.
# $when is a relative time such as "5 min ago".
data-usage-last-recorded = Last recorded { $when }.
data-usage-enter-dates = Enter a start and end date to see that range.
data-usage-nothing-in-period = Nothing was received or sent in this period.
data-usage-err-read = Could not read the data usage. Try again.
data-usage-unavailable = Data usage history is not available here.
data-usage-not-running = The data usage recorder is not running.
data-usage-today = Today
data-usage-this-week = This week
data-usage-this-month = This month
data-usage-this-cycle = This billing cycle
data-usage-billing-cycle = Billing cycle
data-usage-chosen-dates = Chosen dates
data-usage-the-chosen-dates = The chosen dates
data-usage-since-boot = Since boot
data-usage-period-week = Week
data-usage-period-month = Month
data-usage-period-cycle = Cycle
data-usage-period-custom = Custom
data-usage-err-number = Enter a number, such as 10 or 2.5.
data-usage-err-size-zero = Enter a size greater than zero.
data-usage-err-size-smaller = Enter a smaller size.
data-usage-err-date-form = Use the form 2026-10-26.
data-usage-err-date-exists = Enter a date that exists, such as 2026-10-01.
data-usage-err-range-order = Choose an end date after the start date.
data-usage-err-range-length = Choose a range of one year or less.
data-usage-just-now = just now
data-usage-minutes-ago = { $count } min ago
data-usage-hours-ago = { $count } h ago
# $count (Number) - whole days since then.
data-usage-days-ago = { $count ->
    [one] 1 day ago
   *[other] { $count } days ago
}
# $used and $limit are sizes such as "12 GB".
data-usage-over-limit-message = Over the monthly limit: { $used } used of { $limit } this billing cycle.
data-usage-err-save-limit = Could not save the data limit. Try again.
data-usage-limit = Limit
data-usage-cycle-day = Billing cycle starts on day
data-usage-monthly-limit = Monthly limit
data-usage-limit-size = Limit size
data-usage-unit = Unit
data-usage-warn-at = Warn at
data-usage-use-global = Use the limit for all connections
