# Control Centre interaction system

The Control Centre is a quick-control surface. Full setup remains in system
settings. Each detail page uses the shared menu shell and the same hierarchy:

1. Back and page title. A service-wide switch belongs here.
2. The primary control, when present (Sound's Output/Input choice and level).
3. Named groups of compact device or connection rows.
4. One quiet system-settings link, in a fixed footer.

Rows sit flush against one group surface, with no inset around their hover background. Use an
icon and name; add a second line only for meaningful state such as Connected,
Connecting or an unsecured network. A checkmark identifies the selected device.
Do not repeat selection as both a checkmark and a sentence. Hover fades a fixed
colour layer; keyboard focus uses a separate outline.

A row selects or connects an available device. Disconnecting the current network
requires its explicit Disconnect button. Setup actions use an ellipsis and open
system settings. Background discovery does not change the user's connection.

Network shows the physical connection in use, then nearby Wi-Fi. Saved profiles
and VPN interfaces remain available under Other connections. Connected Wi-Fi
must not be repeated in the nearby list. Signal strength is shown by the icon,
not a raw percentage. Container bridges and loopback interfaces belong in full
settings, not in quick controls.

Bluetooth shows My devices while enabled. Add a device starts discovery and
reveals the nearby group; leaving stops discovery owned by this page. The Off
state replaces the device list rather than leaving disabled controls everywhere.

Sound uses Output and Input tabs. Each shows one level control and one device
list. Microphone mute is independent from speaker mute. Switching tabs does not
change either device, mute state or volume. Unavailable hardware produces one
short empty state and a disabled level control.

The footer and notification area stay in place during detail navigation. Motion
uses the shared popup animation and restrained horizontal page transitions.
Reduced motion remains supported.

## Standard components and click conventions

Use the existing components before adding another local control implementation:

- `ShellPopup`: menu hosting, opening/closing, focus and outside dismissal.
- `ControlRow`: one row layout for navigation, selection and quick controls.
  The body navigates/selects; its switch or named secondary action is independent.
- `ControlSwitch`: the same size, motion and focus treatment everywhere.
- `IconButton`: mute, back and other labelled icon actions, including tooltips.
- `ActionButton`: ordinary text actions.
- `ControlCentreButtonSurface`: the common hover, pressed and focus layers.
- `ShellTooltip` and `MediaControls`: shared explanations and playback controls.

Compose these primitives for page-specific layouts. Do not introduce another
switch, device-row or icon-button variant for a new page. Extend an existing
primitive only when its interaction semantics require it; content and backend
behaviour remain at the call site.

The overview has no title, close button or Display shortcut. Volume and Microphone
use the shared AudioLevel component, each with a slider and independent mute.
Each compact row uses an icon, slider, percentage and arrow, without a visible label.
The arrows open the sliding Output and Input device panels respectively; tooltips
and accessible names identify the controls.
Missing devices disable their slider and mute button, while device selection remains
accessible. Overview connectivity rows are passive: the Bluetooth switch changes power,
and separate arrow buttons open network and Bluetooth settings. The row itself
has no hover highlight or navigation action. The top bar places the user avatar on the left and Settings/Lock icon buttons
on the right; routine controls stay on the page.

SegmentedControl owns the inset, spacing, selection and focus styling for audio
tabs and terminal sidebar placement. Consumers supply values and optional glyphs.
Scrollable lists place their scrollbar in the popup's existing outer padding,
without subtracting from row width. Switching pages or crossing the overflow
threshold must not move labels, switches or navigation arrows sideways.
Device lists soften only overflowing edges. Long row names and secondary text
use MarqueeText after a short hover dwell, also available with keyboard focus;
leaving resets the text, and reduced motion retains the static elided label.
MediaControls offers player navigation when multiple MPRIS players are present;
the control centre retains the user's selection until that player disappears.

Control Centre and notification history are independent popups with separate top-bar
buttons. The bell opens floating notification cards and Clear all beneath them;
no enclosing panel or empty-state box is shown. Opening Control Centre does not
open or reposition notifications. Both use the shared ShellPopup motion primitive.
Switches and icon buttons provide hover, pressed and keyboard-focus feedback
without adding a hover background to passive connectivity rows.

Audio sliders remain fixed beneath Output/Input tabs. Only the device list scrolls
and receives edge fades and a scrollbar gutter.

Control Centre smoothly follows the active page's natural height, capped at 80%
of the monitor and above the dock safe boundary. Device scrolling is a fallback
only when the content exceeds that space. Hover colours and button hover opacity
change immediately, without interpolation.

Height changes animate only while the popup is fully open. Closed and opening
popups use their final content size so enabling optional controls does not cause
a second growth animation on the next opening.

VPN shows detected Mullvad, Tailscale and NetworkManager VPN connections, with
independent connection switches in its detail page. Tailscale distinguishes a
private network connection from an active exit node. Its taller detail page
uses a 560px content height, subject to the existing screen and dock limit.
Devices contains tailnet machines, including machines that offer an exit route;
provider servers such as Mullvad appear only under Exit nodes. Exit servers can
be filtered by source and searched by city, country, server name or address.
The current route and Stop using action stay above the list. Exit changes use a
fixed confirmation area, so selection does not move the server rows. Devices
have Copy IP feedback, and the page has its own connection switch. Unchanged
polls preserve scroll position, and offline exit nodes cannot be applied.
The device and exit-node lists reuse DataTable and TableRowSurface from the
process and service views. They use 34px rows, sortable headers, status dots,
four-row wheel steps and Shift+wheel for additional columns. Device rows copy
their address; exit rows select a candidate for the existing confirmation area. Do Not Disturb follows the
desktop banner setting and preserves notification history. Customise controls
persists which rows are shown; adding a row never enables its feature. Night
Light, Power mode and Keep Awake are opt-in. Keep Awake inhibits idle and suspend
only for the current shell session, and stays visible while enabled.
The overview uses a mixed-size grid: larger connectivity tiles, compact toggle
tiles, and short full-width tiles for VPN, power mode and Keep Awake. Shared
ControlRow variants retain independent switches and navigation. Accent icons and
switches communicate state without filling whole tiles blue. Detail pages use flat lists, a single selected tab surface, and
keyboard-only focus outlines. Shared controls retain immediate hover feedback.

The top-bar button uses status indicators without an additional controls glyph.
Optional indicators follow the same visibility preferences as the controls, use
accent colour for active state and muted colour for inactive state, and expose
their state in the tooltip. Status monitoring continues while the bar is visible,
even with Control Centre closed.

Notification history is saved atomically under the shell state directory and
restored into the same model used by dock menus. Restored actions are omitted
after process restarts because the original D-Bus handles are no longer valid.
Avatar previews are 40px beside the text; explicit screenshot previews remain
below it. Image previews are cached locally when rendered.

Notification actions wrap within the card, invoke independently of the default
card action, and update live. Expanded groups lay out each real card at its own
natural height, including full message text, previews and action rows.
