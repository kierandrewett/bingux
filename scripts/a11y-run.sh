#!/usr/bin/env bash
# Run an app with the accessibility bus on and dump its AT-SPI tree.
#
# usage: scripts/a11y-run.sh --app NAME --out FILE [--wait S] [--do SNIPPET] [--wl-run ARGS] -- CMD...
#
# Starts a private session bus (dbus-run-session), the AT-SPI bus launcher, and
# switches `org.a11y.Status.IsEnabled` on, as a desktop does when a screen
# reader runs. accesskit_unix (inside Freya's winit backend) only serves the
# tree while that is on. Then runs CMD, waits, runs the optional --do snippet
# (wtype keys, swaymsg pointer moves) and writes scripts/atspi-tree.py's dump of
# the app named NAME to FILE. BINGUX_A11Y_MIN_TARGET=24 (44 at phone width)
# also lists focusable controls smaller than that.
#
# With WAYLAND_DISPLAY set the app opens on that display; without it the run
# goes through `wl-run` (a private headless sway), and --wl-run passes it
# extra arguments such as "--size 390x800 --shot /tmp/a.png".
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
app=""
out=""
wait=6
snippet=""
wlargs=""
while [ $# -gt 0 ]; do
    case "$1" in
        --app)
            app=$2
            shift 2
            ;;
        --out)
            out=$2
            shift 2
            ;;
        --wait)
            wait=$2
            shift 2
            ;;
        --do)
            snippet=$2
            shift 2
            ;;
        --wl-run)
            wlargs=$2
            shift 2
            ;;
        --)
            shift
            break
            ;;
        *)
            echo "a11y-run: unknown argument $1" >&2
            exit 64
            ;;
    esac
done
[ -n "$app" ] && [ -n "$out" ] && [ $# -gt 0 ] || {
    sed -n 4p "$0" >&2
    exit 64
}

if [ -z "${BINGUX_A11Y_BUS:-}" ]; then
    export BINGUX_A11Y_BUS=1
    exec dbus-run-session -- "$0" --app "$app" --out "$out" --wait "$wait" --do "$snippet" --wl-run "$wlargs" -- "$@"
fi

launcher=""
for candidate in /usr/libexec/at-spi-bus-launcher /usr/lib/at-spi2-core/at-spi-bus-launcher /usr/lib/at-spi-bus-launcher; do
    [ -x "$candidate" ] && launcher=$candidate && break
done
[ -n "$launcher" ] || {
    echo "a11y-run: at-spi-bus-launcher not found (install at-spi2-core)" >&2
    exit 69
}
"$launcher" --launch-immediately >/dev/null 2>&1 &
bus() { dbus-send --session --print-reply --dest=org.a11y.Bus /org/a11y/bus "$@" >/dev/null 2>&1; }
for _ in $(seq 50); do
    bus org.a11y.Bus.GetAddress && break
    sleep 0.1
done
bus org.freedesktop.DBus.Properties.Set string:org.a11y.Status string:IsEnabled variant:boolean:true
bus org.freedesktop.DBus.Properties.Set string:org.a11y.Status string:ScreenReaderEnabled variant:boolean:true

dump="python3 '$here/atspi-tree.py' --app '$app' --out '$out' ${BINGUX_A11Y_MIN_TARGET:+--min-target $BINGUX_A11Y_MIN_TARGET} >/dev/null"
if [ -n "${WAYLAND_DISPLAY:-}" ]; then
    "$@" &
    pid=$!
    sleep "$wait"
    [ -n "$snippet" ] && bash -c "$snippet"
    bash -c "$dump"
    kill "$pid" 2>/dev/null || true
else
    # shellcheck disable=SC2086 # extra wl-run arguments are words
    wl-run --wait "$wait" $wlargs --do "${snippet:+$snippet; }$dump" -- "$@"
fi
echo "a11y-run: wrote $out ($(wc -l <"$out") lines)"
