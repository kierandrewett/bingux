#!/usr/bin/env bash
# Run on an isolated Wayland display; inherits its GTK settings and renderer.
set -euo pipefail
package="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
output="${1:-$(mktemp -d /tmp/bingux-title-test.XXXXXX)}"
mkdir -p "$output"
read -r -a gtk_flags <<<"$(pkg-config --cflags --libs gtk4 libadwaita-1)"
cc -Wall -Wextra -Wno-unused-parameter -Wno-deprecated-declarations \
    -I"$package/vendor" "$package/test-title-rendering.c" \
    -o "$output/test-title-rendering" "${gtk_flags[@]}" -lm
for width in 521 522 523; do
    for state in 60 61; do
        "$output/test-title-rendering" "$output" "$width" "$state"
    done
done
echo "Title comparison images: $output"
