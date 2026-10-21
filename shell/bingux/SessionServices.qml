pragma Singleton
import QtQuick

// Capability gate for actions a widget may take. In the desktop everything
// is allowed. The lock screen and the greeter run the same widget code, so
// every path that launches, shows content or reaches a session service asks
// here first and refuses when the context does not permit it.
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
//   volume              change the output volume
QtObject {
    id: root

    readonly property var table: ({
            locked: ["volume", "power"],
            greeter: ["power"]
        })

    function allows(capability) {
        if (!SessionContext.restricted)
            return true;
        const granted = table[SessionContext.mode] || [];
        if (granted.includes(capability))
            return true;
        console.log("[session] " + capability + " refused in " + SessionContext.mode + " context");
        return false;
    }
}
