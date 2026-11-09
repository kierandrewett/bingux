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

## Mouse

settings-mouse-speed-read-error = Could not read the pointer speed. Restart Settings and try again.
settings-mouse-read-error = Could not read the mouse settings. Restart Settings and try again.
settings-mouse-speed-slow = Slow
settings-mouse-speed-slower = Slower than normal
settings-mouse-speed-normal = Normal
settings-mouse-speed-faster = Faster than normal
settings-mouse-speed-fast = Fast
settings-mouse-millis = { $ms } ms
settings-mouse-set-to = Set to { $value }.
settings-mouse-accel-default = Default
settings-mouse-accel-flat = Flat
settings-mouse-accel-adaptive = Adaptive
settings-mouse-send-always = Always on
settings-mouse-send-external = Off when a mouse is connected
settings-mouse-off = Off
settings-mouse-primary-right = Right
settings-mouse-primary-left = Left
settings-mouse-primary-mouse = Mouse
settings-mouse-click-automatic = Automatic
settings-mouse-click-fingers = Two fingers
settings-mouse-click-areas = Bottom corners
settings-mouse-double-click-detected = Double-click detected
settings-mouse-double-click-try = Double-click this box
settings-mouse-scroll-line = Line { $n }
settings-mouse-try-again = Try again
settings-mouse-group = Mouse
settings-mouse-touchpad-group = Touchpad
settings-mouse-test-group = Test your settings
settings-mouse-natural-scroll = Natural scrolling
settings-mouse-pointer-speed = Pointer speed
settings-mouse-left-handed = Left-handed buttons
settings-mouse-pointer-acceleration = Pointer acceleration
settings-mouse-double-click-speed = Double-click speed
settings-mouse-middle-click-paste = Middle click paste
settings-mouse-use-touchpad = Use touchpad
settings-mouse-two-finger = Two-finger
settings-mouse-edge = Edge
settings-mouse-tap-to-click = Tap to click
settings-mouse-secondary-click = Secondary click
settings-mouse-scrolling = Scrolling
settings-mouse-primary-button = Primary button
settings-mouse-ignore-typing = Ignore touchpad while typing

## Privacy

settings-privacy-level-default = Use default
settings-privacy-level-ask = Ask
settings-privacy-level-allow = Allow
settings-privacy-level-deny = Deny
settings-privacy-capability-screen-cast = Screen cast
settings-privacy-capability-remote-desktop = Remote desktop
settings-privacy-capability-input-capture = Input capture
settings-privacy-capability-screenshot = Screenshot
settings-privacy-capability-access = Access
settings-privacy-device-keyboard = Keyboard
settings-privacy-device-pointer = Pointer
settings-privacy-device-touchscreen = Touchscreen
settings-privacy-selector-app = App { $id }
settings-privacy-selector-program = Program { $path }
settings-privacy-selector-pattern = Pattern { $pattern }
settings-privacy-rule-summary = { $selectors } · { $capabilities } · { $level }
settings-privacy-rule-missing = Could not find that rule. Reopen the page and try again.
settings-privacy-save-error = Could not save the rule. Check that its name is not already used, then try again.
settings-privacy-move-error = Could not move the rule. Reopen the page and try again.
settings-privacy-name-required = Enter a name for the rule.
settings-privacy-capability-required = Choose at least one capability.
settings-privacy-selector-required = Enter an app ID, a program path or a pattern.
settings-privacy-path-absolute = Start the program path with /.
settings-privacy-try-again = Try again
settings-privacy-fix-file = Fix or remove the permissions file, then try again.
settings-privacy-default-group = Default
settings-privacy-default-level = Default level
settings-privacy-default-help = Used when no rule applies. Deny refuses every request that no rule allows.
settings-privacy-no-file = No permission file yet
settings-privacy-no-file-help = Requests use the default level. Saving a rule creates the file.
settings-privacy-rules-group = Rules
settings-privacy-no-rules = No rules
settings-privacy-no-rules-help = Requests use the default level. Add a rule to allow or deny a specific app.
settings-privacy-new-rule = New rule
settings-privacy-new-rule-help = A matching deny always wins. Otherwise the rule listed last wins.
settings-privacy-add-rule = Add rule
settings-privacy-edit-rule = Edit rule
settings-privacy-delete-rule = Delete rule
settings-privacy-move-up = Move rule up
settings-privacy-move-down = Move rule down
settings-privacy-delete-title = Delete “{ $name }”?
settings-privacy-delete-body = Apps that match this rule use the other rules and the default level.
settings-privacy-cancel = Cancel
settings-privacy-delete = Delete
settings-privacy-field-name = Name
settings-privacy-field-app-id = App ID
settings-privacy-field-app-id-help = The app ID, such as org.example.Viewer.
settings-privacy-field-program = Program
settings-privacy-field-program-help = The full path, such as /usr/bin/viewer.
settings-privacy-field-pattern = Pattern
settings-privacy-field-pattern-help = Use * for any text and ? for one character.
settings-privacy-field-monitors = Monitors
settings-privacy-field-monitors-help = Monitor names separated by commas, such as DP-1.
settings-privacy-field-clipboard = Clipboard
settings-privacy-field-clipboard-help = Remote desktop sessions may use the clipboard.
settings-privacy-field-level = Level
settings-privacy-save = Save
