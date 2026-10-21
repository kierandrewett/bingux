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
if [[ -n "$(git -C "$root" status --porcelain --untracked-files=no)" ]]; then
    if [[ "${BINGUX_ARCHIVE_FROM_HEAD:-0}" != 1 ]]; then
        echo "working tree has changes; commit the release source first" >&2
        echo "(BINGUX_ARCHIVE_FROM_HEAD=1 archives the committed HEAD and ignores the edits)" >&2
        exit 1
    fi
    echo "warning: BINGUX_ARCHIVE_FROM_HEAD=1: uncommitted tracked edits are ignored;" >&2
    echo "warning: archiving git HEAD $(git -C "$root" rev-parse --short HEAD)" >&2
fi
# Every pinned submodule (the Adwaita icon theme, Quickshell for bingux-shell)
# ships inside the archive at its pinned revision; package builds run offline.
mapfile -t submodules < <(git -C "$root" config --file .gitmodules --get-regexp '^submodule\..*\.path$' | awk '{print $2}')
for path in "${submodules[@]}"; do
    submodule="$root/$path"
    expected="$(git -C "$root" rev-parse "HEAD:$path" 2>/dev/null || true)"
    if [[ -z "$expected" || ! -d "$submodule/.git" && ! -f "$submodule/.git" ]]; then
        echo "submodule $path is missing; run git submodule update --init" >&2
        exit 1
    fi
    actual="$(git -C "$submodule" rev-parse HEAD)"
    if [[ "$actual" != "$expected" || -n "$(git -C "$submodule" status --porcelain)" ]]; then
        echo "submodule $path does not match the pinned source revision" >&2
        exit 1
    fi
done

archive_root="$(mktemp -d)"
trap 'rm -rf "$archive_root"' EXIT
archive_prefix="bingux-${version}"
git -C "$root" archive --format=tar --prefix="${archive_prefix}/" HEAD |
    tar -xf - -C "$archive_root"
for path in "${submodules[@]}"; do
    mkdir -p "$archive_root/$archive_prefix/$path"
    git -C "$root/$path" archive --format=tar HEAD |
        tar -xf - -C "$archive_root/$archive_prefix/$path"
done
tar -C "$archive_root" -cf - "$archive_prefix" | xz -T0 -9 >"$root/$output"
echo "$root/$output"
