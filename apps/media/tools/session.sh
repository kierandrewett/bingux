#!/bin/sh
# Run COMMAND in a fresh session bus with a PulseAudio null sink (no real audio device).
# Used by the a37 box harness to drive bingux-media for screenshots and MPRIS checks.
# Usage: session.sh COMMAND [ARGS...]
export HOME=${HOME:-/root}
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
exec dbus-run-session -- sh -c '
  pulseaudio --start --exit-idle-time=-1 --system=false >/tmp/pulse.log 2>&1
  pactl load-module module-null-sink sink_name=null sink_properties=device.description=Null >/dev/null
  pactl set-default-sink null
  exec "$@"' sh "$@"
