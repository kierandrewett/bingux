#!/usr/bin/env bash
# Build the release source archive: bingux-VERSION.tar.xz.
#
# usage: scripts/make-source-archive.sh [VERSION [OUTPUT]]
#
# The archive holds HEAD plus the pinned Adwaita icon theme (a submodule that
# `git archive` does not follow). It is reproducible: files are sorted, owned by
# root and stamped with the commit time (or SOURCE_DATE_EPOCH when set), so the
# same commit gives the same bytes.
#
# Release archives must come from a clean checkout. For a packaging check of
# work that is not committed yet, set BINGUX_ARCHIVE_WORKTREE=1: the archive
# then holds the working tree (tracked and untracked files that are not
# ignored) as it is now. Never publish such an archive.
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

worktree="${BINGUX_ARCHIVE_WORKTREE:-0}"
if [[ "$worktree" != 1 && -n "$(git -C "$root" status --porcelain)" ]]; then
    echo "working tree has changes; commit the release source first" >&2
    echo "(BINGUX_ARCHIVE_WORKTREE=1 archives the working tree for a local packaging check)" >&2
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

epoch="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"
archive_root="$(mktemp -d)"
trap 'rm -rf "$archive_root"' EXIT
archive_prefix="bingux-${version}"
mkdir -p "$archive_root/$archive_prefix"

if [[ "$worktree" == 1 ]]; then
    # Tracked and untracked files that exist now; a file deleted in the working
    # tree is still listed by the index, so skip what is gone.
    (
        cd "$root"
        git ls-files -z --cached --others --exclude-standard --deduplicate -- . ':!subprojects/adwaita-icon-theme' |
            while IFS= read -r -d '' file; do
                [[ -e "$file" || -L "$file" ]] && printf '%s\0' "$file"
            done |
            tar --null --no-recursion -T - -cf -
    ) | tar -xf - -C "$archive_root/$archive_prefix"
else
    git -C "$root" archive --format=tar --prefix="${archive_prefix}/" HEAD |
        tar -xf - -C "$archive_root"
fi
mkdir -p "$archive_root/$archive_prefix/subprojects/adwaita-icon-theme"
git -C "$submodule" archive --format=tar HEAD |
    tar -xf - -C "$archive_root/$archive_prefix/subprojects/adwaita-icon-theme"

# OUTPUT is relative to the checkout unless it is an absolute path.
[[ "$output" == /* ]] || output="$root/$output"
mkdir -p "$(dirname "$output")"
tar -C "$archive_root" --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${epoch}" \
    -cf - "$archive_prefix" | xz -T0 -9 >"$output"
echo "$output"
