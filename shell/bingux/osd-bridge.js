import Gio from "gi://Gio";
import GLib from "gi://GLib";
import * as Main from "resource:///org/gnome/shell/ui/main.js";

// Old running Gnoblin builds suppress native OSDs without emitting the v2
// event. Supply that event from the same shell connection until the next login
// uses a build with native support. Never install over the native v2 bridge.
export default function enable(api) {
    const connection = Gio.DBus.session;
    const manager = Main.osdWindowManager;
    const settings = new Gio.Settings({ schema_id: "org.gnoblin.shell" });
    let disposed = false;
    let original = null;
    let wrapper = null;
    let monitorSignal = 0;
    let outputs = [];

    function refreshOutputs() {
        connection.call(
            "org.gnome.Mutter.DisplayConfig",
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
            null,
            null,
            Gio.DBusCallFlags.NONE,
            1500,
            null,
            (bus, result) => {
                if (disposed) return;
                try {
                    const logical = bus.call_finish(result).deepUnpack()[2];
                    outputs = Main.layoutManager.monitors.map(
                        (monitor) =>
                            logical
                                .find((item) => item[0] === monitor.x && item[1] === monitor.y)?.[5]
                                .map((spec) => spec[0]) ?? [],
                    );
                } catch (error) {
                    console.warn(`bingux-osd: output discovery failed: ${error.message}`);
                }
            },
        );
    }

    connection.call(
        "org.gnoblin.Shell",
        "/org/gnoblin/Shell",
        "org.freedesktop.DBus.Introspectable",
        "Introspect",
        null,
        null,
        Gio.DBusCallFlags.NONE,
        1500,
        null,
        (bus, result) => {
            if (disposed) return;
            try {
                const xml = bus.call_finish(result).deepUnpack()[0];
                if (/<signal\s+name=["']OsdRequested["']/.test(xml)) return;
                refreshOutputs();
                monitorSignal = Main.layoutManager.connect("monitors-changed", refreshOutputs);
                original = manager._showOsdWindow;
                wrapper = function (monitorIndex, icon, label, level, maxLevel) {
                    const disabled = settings.get_strv("disabled-features");
                    if (!disabled.includes("osd"))
                        return original.call(this, monitorIndex, icon, label, level, maxLevel);
                    const names = outputs[monitorIndex] ?? [];
                    if (names.length === 0) return;
                    const iconName = typeof icon === "string" ? icon : (icon?.get_names?.()[0] ?? "");
                    const text = String(label ?? "")
                        .replace(/[\u0000-\u001f\u007f]/g, " ")
                        .slice(0, 512);
                    connection.emit_signal(
                        null,
                        "/org/gnoblin/Shell",
                        "org.gnoblin.Shell",
                        "OsdRequested",
                        new GLib.Variant("(uissddas)", [
                            2,
                            monitorIndex,
                            iconName,
                            text,
                            Number.isFinite(level) ? level : -1,
                            Number.isFinite(maxLevel) ? maxLevel : 1,
                            names,
                        ]),
                    );
                };
                manager._showOsdWindow = wrapper;
            } catch (error) {
                console.warn(`bingux-osd: compatibility bridge failed: ${error.message}`);
            }
        },
    );

    api._disposers.push(() => {
        disposed = true;
        if (monitorSignal) Main.layoutManager.disconnect(monitorSignal);
        if (wrapper && manager._showOsdWindow === wrapper) manager._showOsdWindow = original;
    });
}
