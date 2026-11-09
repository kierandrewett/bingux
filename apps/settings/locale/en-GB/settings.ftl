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
gnoblin-managed-subtitle = Set in your desktop configuration
gnoblin-managed-badge = Managed
gnoblin-settings-read-error = Could not read the desktop settings. Restart the desktop and try again.
sound-volume-read-error = Could not read the volume. Try again in a moment.
