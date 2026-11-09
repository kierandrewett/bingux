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
