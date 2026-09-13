#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
output="${1:-$root/build}"
sdk="${GNOBLIN_FRAME_SDK:-$root/packages/bingux-frame/vendor}"
protocol="${GNOBLIN_FRAME_PROTOCOL:-$root/packages/bingux-frame/vendor/gnoblin-window-frame-v1.xml}"
mkdir -p "$output/frame"
wayland-scanner client-header "$protocol" "$output/frame/gnoblin-window-frame-v1-client-protocol.h"
wayland-scanner private-code "$protocol" "$output/frame/protocol.c"
compiler_flags="$(pkg-config --cflags --libs gtk4 libadwaita-1 wayland-client)" || exit 1
read -r -a compiler_args <<<"$compiler_flags"
cc -std=c11 -Wall -Wextra -Wno-unused-parameter -Wno-deprecated-declarations \
    -I"$sdk" -I"$output/frame" "$sdk/client.c" "$output/frame/protocol.c" \
    "$root/packages/bingux-frame/paint-gtk.c" -o "$output/bingux-frame" \
    "${compiler_args[@]}" -lm
