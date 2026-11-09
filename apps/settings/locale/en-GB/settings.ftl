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

## VPN
# VPN connection state, shown on the row.
network-vpn-state-connected = Connected
# VPN connection state, shown on the row. Variable: address, the VPN's address.
network-vpn-state-connected-at = Connected · { $address }
network-vpn-state-connecting = Connecting…
# VPN connection state, shown on the row. Variable: reason, why it failed.
network-vpn-state-failed = Failed: { $reason }
network-vpn-state-off = Not connected
# Dialog title. Variable: name, the VPN's name.
network-vpn-delete-title = Delete “{ $name }”?
network-vpn-delete-body = This removes the VPN and the keys and passwords saved in it.
network-vpn-needs-connection = Needs a connection
network-vpn-status-error = Could not read the network status. Try again in a moment.
network-vpn-add-title = Add VPN
network-vpn-import-title = Import VPN
# Page title for a VPN type. Variable: name, the VPN type's name.
network-vpn-add-kind-title = Add { $name }
network-vpn-add-name-taken = Use a name that no other connection has.
network-vpn-add-type = Type
network-vpn-add-from-file = From a file
network-vpn-add-import-from-file = Import from file…
network-vpn-add-name-required = Give the VPN a name.
network-vpn-add-group-interface = Interface
network-vpn-add-private-key = Private key
network-vpn-add-key-local = Stays on this computer. Generate one, or paste your own.
network-vpn-add-key-provider = Paste the key from your VPN provider.
network-vpn-add-generate-key = Generate key
network-vpn-add-addresses = Addresses
network-vpn-add-addresses-hint = This computer's addresses inside the tunnel, one per line, with the prefix length.
network-vpn-add-dns = DNS servers
network-vpn-add-dns-hint = Used while the tunnel is up, one per line.
network-vpn-add-group-peer = Peer
network-vpn-add-public-key = Public key
network-vpn-add-public-key-hint = The other side's public key.
network-vpn-add-endpoint = Endpoint
network-vpn-add-endpoint-hint = Where to reach it, as host:port.
network-vpn-add-preshared = Preshared key
network-vpn-add-preshared-hint = An extra secret both sides share, if they use one.
network-vpn-add-keepalive = Keepalive
# Suffix after the keepalive field.
network-vpn-add-seconds = seconds
network-vpn-add-keepalive-hint = Keeps the connection open through a router. 25 is usual.
network-vpn-add-group-traffic = Traffic
network-vpn-add-route-all = Route all traffic
network-vpn-add-allowed = Allowed addresses
network-vpn-add-allowed-hint = Networks that go through this peer, one per line.
network-vpn-add-name = Name
network-vpn-add-name-hint = How this VPN is listed.
network-vpn-add-submit = Add VPN
network-vpn-add-no-connect = It does not connect until you turn it on.
network-vpn-add-server = Server
network-vpn-add-settings-needed = Add the settings this VPN needs, one name = value per line.
network-vpn-add-settings = Settings
network-vpn-add-settings-hint = One name = value per line.
# Error. Variable: error, the reason the file could not be read.
network-vpn-add-read-failed = The file could not be read: { $error }.
network-vpn-add-unknown-kind = This file does not say what kind of VPN it is. Choose the type above.
# Error. Variable: name, the VPN type's name.
network-vpn-add-not-installed = { $name } is not installed. Install its plugin to add this VPN.
network-vpn-add-new-missing = The new VPN could not be found. Try again.
network-vpn-add-filter-vpn = VPN configuration
network-vpn-add-choose-file = Choose a VPN file
network-vpn-add-filter-all = All files
network-vpn-add-path-required = Enter the full path of the file, starting with /.
network-vpn-add-detect = Detect from the file
# Hint listing the file types a plugin reads. Variable: list, the file extensions.
network-vpn-add-reads = Reads { $list }.
network-vpn-add-file = File
network-vpn-add-browse-hint = Or choose the file from your folders.
network-vpn-add-browse = Browse…
network-vpn-add-import = Import
network-vpn-add-import-note = The VPN opens when it is imported. It does not connect until you turn it on.
network-vpn-add-failed = Could not add the VPN. Check the details and try again.
network-vpn-error-host-port = Enter the server as a host name or address, such as vpn.example.com or vpn.example.com:1194.
network-vpn-error-absolute-path = Enter the full path of the file, starting with /.
network-vpn-field-your-private-key = Your private key
network-vpn-read-failed = Could not read this VPN. Try again.
network-vpn-error-cidr-required = Enter at least one address with its prefix length, such as 10.8.0.2/32.
# Error. Variable: item, the address that is missing its prefix length.
network-vpn-error-cidr-item = “{ $item }” needs a prefix length, such as 10.8.0.2/32.
network-vpn-error-dns-required = Enter at least one IP address, one per line.
# Error. Variable: item, the entry that is not an IP address.
network-vpn-error-not-ip = “{ $item }” is not an IP address.
network-vpn-error-endpoint = Enter the server as host:port, such as vpn.example.com:51820 or [2001:db8::1]:51820.
network-vpn-error-mtu = Enter a size from 68 to 65535, or leave it blank.
network-vpn-error-fwmark = Enter a number such as 51820, or leave it empty.
network-vpn-error-keepalive = Enter a number of seconds from 0 to 65535, or leave it blank.
network-vpn-key-generate-failed = Could not generate a key. Try again.
network-vpn-import-space = Could not import the VPN. Check the free space and try again.
network-vpn-import-check-file = Could not import the VPN. Check the file and try again.
network-vpn-new-vpn-missing = Could not find the new VPN. Try again.
network-vpn-nmcli-missing = Network settings are not available. Restart the computer and try again.
network-vpn-nmcli-failed = Could not open the network settings. Try again.
network-vpn-reason-needs-secret = The VPN needs a password or key that is not saved. Add it in the VPN's settings.
network-vpn-reason-refused = The server refused the user name or password.
network-vpn-reason-timeout = The server did not answer in time. Check its address and your connection.
network-vpn-reason-program-failed = The VPN program failed to start. Check its settings, and that its plugin is installed.
network-vpn-reason-no-settings = The server sent no usable network settings. Try again later.
network-vpn-reason-no-device = There is no network connection to carry the VPN. Connect to a network first.
network-vpn-reason-gone = This VPN no longer exists.
network-vpn-reason-disconnected = The VPN was disconnected.
network-vpn-reason-not-allowed = You are not allowed to change this connection.
network-vpn-reason-no-reason = The VPN did not connect. Try again.
network-vpn-reason-fallback = Could not change the VPN. Try again.
network-vpn-unreadable = Could not read the VPN settings. Try again in a moment.
network-vpn-peers-missing = Could not read the VPN peers. Restart the computer and try again.
network-vpn-connection-not-found = Could not find this connection. Try again.
network-vpn-connection-unreadable = Could not read this connection. Try again.

# Plugin field labels, shown on the VPN form.
network-vpn-field-server = Server
network-vpn-field-user-name = User name
network-vpn-field-password = Password
network-vpn-field-ca = Server certificate (CA)
network-vpn-field-certificate = Your certificate
network-vpn-field-key-password = Key password
network-vpn-field-static-key-file = Static key file
network-vpn-field-gateway = Gateway
network-vpn-field-protocol = Protocol
network-vpn-field-group-name = Group name
network-vpn-field-group-password = Group password
network-vpn-field-domain = Domain
network-vpn-field-port = Port
network-vpn-field-sign-in-with = Sign in with
network-vpn-field-key-file = Key file

# Plugin choice labels, shown in the VPN form's menus.
network-vpn-choice-certificates = Certificates
network-vpn-choice-password-certificates = Password and certificates
network-vpn-choice-static-key = Static key
network-vpn-choice-ssh-agent = SSH agent

# Save-scope choices for a VPN's secrets.
network-vpn-flags-all-users = Save for all users
network-vpn-flags-this-user = Save for this user only
network-vpn-flags-ask = Ask every time

# Plugin field hints, shown under the VPN form's fields.
network-vpn-hint-server-port = Host name or address, with a port if it is not the default.
network-vpn-hint-gateway = Host name or address of the VPN server.
network-vpn-hint-ssh-server = Host name or address of the SSH server.
