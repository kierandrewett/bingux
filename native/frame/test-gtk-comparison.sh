#!/usr/bin/env bash
# Inherit the normal-config nested display and GTK theme.
set -euo pipefail
package="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
output="${1:?pass a directory for the comparison frames}"
mkdir -p "$output"
read -r -a gtk_flags <<<"$(pkg-config --cflags --libs gtk4 libadwaita-1)"
cc -Wall -Wextra -Wno-unused-parameter -Wno-deprecated-declarations \
    -I"$package/vendor" "$package/test-gtk-comparison.c" \
    -o "$output/compare" "${gtk_flags[@]}" -lm
"$output/compare" "$output" "${@:2}"
