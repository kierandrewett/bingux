#!/usr/bin/env bash
# Runs only on its own headless display and isolated settings bus.
set -euo pipefail
package="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
output="${1:?pass an output directory}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
read -r -a gtk_flags <<<"$(pkg-config --cflags --libs gtk4 libadwaita-1)"
cc -O2 -Wall -Wextra -Wno-unused-parameter -Wno-deprecated-declarations \
    -I"$package/vendor" "$package/test-button-layout.c" \
    -o "$output/test-button-layout" "${gtk_flags[@]}" -lm
runtime="$(mktemp -d /tmp/bingux-layout-runtime.XXXXXX)"
weston_pid=""
cleanup() {
    if [[ -n "$weston_pid" ]]; then
        kill "$weston_pid" 2>/dev/null || true
        wait "$weston_pid" 2>/dev/null || true
    fi
    rm -rf "$runtime"
}
trap cleanup EXIT
XDG_RUNTIME_DIR="$runtime" weston --backend=headless --renderer=pixman --no-config \
    --socket=layout-test --idle-time=0 --log="$output/weston.log" >"$output/weston.stdout" 2>&1 &
weston_pid=$!
for _ in {1..100}; do
    [[ -S "$runtime/layout-test" ]] && break
    kill -0 "$weston_pid"
    sleep 0.05
done
[[ -S "$runtime/layout-test" ]]
cat >"$runtime/bus.conf" <<'EOF'
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
for override in "" "--button-layout=:close"; do
    XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY=layout-test GDK_BACKEND=wayland \
        GTK_A11Y=none GIO_USE_VFS=local GSETTINGS_BACKEND=memory GSK_RENDERER=gl \
        dbus-run-session --config-file="$runtime/bus.conf" -- \
        "$output/test-button-layout" --compact "$override"
done | tee "$output/results.txt"
