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

## Gnoblin

settings-gnoblin-not-running = The desktop is not running. Sign out and sign in again.
settings-gnoblin-managed = Set in your desktop configuration
settings-gnoblin-managed-badge = Managed
settings-gnoblin-on = On
settings-gnoblin-off = Off
settings-gnoblin-custom = Custom
settings-gnoblin-millis = { $ms } ms
settings-gnoblin-read-error = Could not read the desktop settings. Restart the desktop and try again.
