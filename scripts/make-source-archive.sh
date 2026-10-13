#!/usr/bin/env bash
set -euo pipefail

version="${1:-0.1.0}"
output="${2:-dist/bingux-${version}.tar.xz}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

case "$version" in
    *[!0-9.]* | .* | *.)
        echo "version must contain only digits and dots" >&2
        exit 2
        ;;
esac

mkdir -p "$(dirname "$root/$output")"
if [[ -n "$(git -C "$root" status --porcelain)" ]]; then
    echo "working tree has changes; commit the release source first" >&2
    exit 1
fi
submodule="$root/subprojects/adwaita-icon-theme"
expected_submodule="$(git -C "$root" rev-parse HEAD:subprojects/adwaita-icon-theme 2>/dev/null || true)"
if [[ -z "$expected_submodule" || ! -d "$submodule/.git" && ! -f "$submodule/.git" ]]; then
    echo "Adwaita icon theme submodule is missing; run git submodule update --init" >&2
    exit 1
fi
actual_submodule="$(git -C "$submodule" rev-parse HEAD)"
if [[ "$actual_submodule" != "$expected_submodule" || -n "$(git -C "$submodule" status --porcelain)" ]]; then
    echo "Adwaita icon theme checkout does not match the pinned source revision" >&2
    exit 1
fi

archive_root="$(mktemp -d)"
trap 'rm -rf "$archive_root"' EXIT
archive_prefix="bingux-${version}"
git -C "$root" archive --format=tar --prefix="${archive_prefix}/" HEAD |
    tar -xf - -C "$archive_root"
mkdir -p "$archive_root/$archive_prefix/subprojects/adwaita-icon-theme"
git -C "$submodule" archive --format=tar HEAD |
    tar -xf - -C "$archive_root/$archive_prefix/subprojects/adwaita-icon-theme"
tar -C "$archive_root" -cf - "$archive_prefix" | xz -T0 -9 >"$root/$output"
echo "$root/$output"
