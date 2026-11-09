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
