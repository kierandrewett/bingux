import Clutter from "gi://Clutter";
import GLib from "gi://GLib";
import Meta from "gi://Meta";
import * as Main from "resource:///org/gnome/shell/ui/main.js";
import { ClipboardPaste } from "./lib/clipboard-paste.js";

function eligible(window) {
    return window && !window.skip_taskbar && !window.is_override_redirect();
}

function windowFor(id) {
    return global
        .get_window_actors()
        .map((actor) => actor.meta_window)
        .find((window) => window && String(window.get_stable_sequence()) === id);
}

export default function enable(api) {
    const clipboardPaste = new ClipboardPaste(global.stage.context.get_backend().get_default_seat());
    let caret = null;
    let pasteOwner = null;
    const focus = global.display.connect("notify::focus-window", () => {
        caret = null;
    });
    const cursorLocation = Main.inputMethod.connect("cursor-location-changed", (_method, rect) => {
        const window = global.display.focus_window;
        const inputFocus = Main.inputMethod.currentFocus;
        if (!window || !inputFocus || Main.sessionMode.isLocked) return;
        const buffer = window.get_buffer_rect();
        // Mutter reports stage coordinates. Keep an offset so a moving window
        // does not leave the insertion anchor at its previous location.
        caret = {
            window,
            inputFocus,
            x: rect.get_x() - buffer.x,
            y: rect.get_y() - buffer.y,
            width: rect.get_width(),
            height: rect.get_height(),
        };
    });

    api.handleCompositorOperation("bingux.input-anchor", (_record, peer) => {
        if (Main.sessionMode.isLocked) throw new Error("Session is locked.");
        const window = global.display.focus_window;
        const [x, y] = global.get_pointer();
        const frame = window?.get_frame_rect();
        const buffer = window?.get_buffer_rect();
        const inputFocus = Main.inputMethod.currentFocus;
        const anchor = caret?.window === window && inputFocus && caret.inputFocus === inputFocus ? caret : null;
        peer.send({
            event: "input-anchor",
            x,
            y,
            caret: anchor
                ? {
                      x: buffer.x + anchor.x,
                      y: buffer.y + anchor.y,
                      width: anchor.width,
                      height: anchor.height,
                      source: "caret",
                  }
                : null,
            pid: window?.get_pid() || 0,
            window: window ? String(window.get_stable_sequence()) : "",
            buffer: buffer ? { x: buffer.x, y: buffer.y, width: buffer.width, height: buffer.height } : null,
            frame: frame ? { x: frame.x, y: frame.y, width: frame.width, height: frame.height } : null,
        });
    });

    api.handleCompositorOperation("bingux.type-text", (record, peer) => {
        const window = typeof record.window === "string" ? windowFor(record.window) : null;
        if (
            Main.sessionMode.isLocked ||
            !eligible(window) ||
            global.display.focus_window !== window
        )
            throw new Error("The original input window is no longer focused.");
        if (
            typeof record.text !== "string" ||
            !record.text.length ||
            record.text.length > 64 ||
            /[\u0000-\u001f\u007f-\u009f]/.test(record.text)
        )
            throw new Error("Invalid text insertion.");
        const modifiers = global.get_pointer()[2];
        const blocked =
            Clutter.ModifierType.CONTROL_MASK |
            Clutter.ModifierType.MOD1_MASK |
            Clutter.ModifierType.MOD4_MASK |
            Clutter.ModifierType.SUPER_MASK;
        if (modifiers & blocked) throw new Error("Release modifier keys before inserting text.");

        if (window.get_client_type() === Meta.WindowClientType.X11) {
            if (pasteOwner !== null) throw new Error("A text insertion is already in progress.");
            pasteOwner = peer.client;
            clipboardPaste
                .paste(record.text, () => {
                    if (!peer.isOpen() || Main.sessionMode.isLocked || global.display.focus_window !== window)
                        throw new Error("The original input window is no longer focused.");
                    if (global.get_pointer()[2] & (blocked | Clutter.ModifierType.SHIFT_MASK))
                        throw new Error("Release modifier keys before inserting text.");
                })
                .then(
                    () => peer.isOpen() && peer.send({ event: "typed", window: record.window }),
                    (error) => peer.isOpen() && peer.send({ event: "error", message: error.message }),
                )
                .finally(() => {
                    if (pasteOwner === peer.client) pasteOwner = null;
                });
            return;
        }

        if (!Main.inputMethod.currentFocus)
            throw new Error("This app does not expose a text input. Focus its input field and try again.");
        Main.inputMethod.commit(record.text);
        peer.send({ event: "typed", window: record.window });
    });

    api.onCompositorClientClosed((client) => {
        if (client === pasteOwner) {
            pasteOwner = null;
            clipboardPaste.destroy();
        }
    });
    api.addCleanup(() => {
        clipboardPaste.destroy();
        global.display.disconnect(focus);
        Main.inputMethod.disconnect(cursorLocation);
    });
}
