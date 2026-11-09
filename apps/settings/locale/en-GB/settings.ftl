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
