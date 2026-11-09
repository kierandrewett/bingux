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
