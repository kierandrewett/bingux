pragma Singleton
import QtQuick

// Capability gate for actions a widget may take. In the desktop everything
// is allowed. The lock screen, the greeter and kiosk hosts run the same
// widget code, so every path that launches, shows content or reaches a
// session service asks here first and refuses when the context does not
// permit it.
//
// Capabilities:
//   launch              start applications or run app commands
//   search              open search
//   trayMenus           activate tray items and their menus
//   power               restart, shut down, suspend
//   settings            open Bingux Settings or GNOME Settings
//   lock                start the lock client
//   notificationContent show notification bodies and actions
//   mediaControl        play, pause, skip
//   volume              change the output volume or mute the microphone
//   network             switch networks, join Wi-Fi
//   doNotDisturb        toggle Do Not Disturb
//
// Before a user session exists (lock, greeter, kiosk) the control centre's
// controls are granted; everything that opens content or launches is refused.
QtObject {
    id: root

    readonly property var preLoginGrants: ["volume", "power", "network", "doNotDisturb"]

    readonly property var table: ({
            locked: preLoginGrants,
            greeter: preLoginGrants,
            kiosk: preLoginGrants
        })

    // Pure check for visibility bindings; does not log.
    function permits(capability) {
        if (!SessionContext.restricted)
            return true;
        return (table[SessionContext.mode] || []).includes(capability);
    }

    function allows(capability) {
        if (permits(capability))
            return true;
        console.log("[session] " + capability + " refused in " + SessionContext.mode + " context");
        return false;
    }
}
