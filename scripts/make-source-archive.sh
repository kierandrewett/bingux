#!/usr/bin/env bash
set -euo pipefail

version="${1:-0.1.0}"
output="${2:-dist/bingux-${version}.tar.xz}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

case "$version" in
    (*[!0-9.]*|.*|*.)
        echo "version must contain only digits and dots" >&2
        exit 2
        ;;
esac

mkdir -p "$(dirname "$root/$output")"
git -C "$root" diff --quiet || {
    echo "working tree has changes; commit the release source first" >&2
    exit 1
}
git -C "$root" archive --format=tar --prefix="bingux-${version}/" HEAD \
    | xz -T0 -9 > "$root/$output"
echo "$root/$output"
