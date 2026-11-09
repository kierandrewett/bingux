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
