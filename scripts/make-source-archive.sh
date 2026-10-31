#!/usr/bin/env bash
# Build the release source archive: bingux-VERSION.tar.xz.
#
# usage: scripts/make-source-archive.sh [VERSION [OUTPUT]]
#
# The archive holds HEAD plus every pinned submodule (Quickshell for bingux-shell;
# `git archive` does not follow submodules). It is reproducible: files are sorted, owned by
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

# A release is digits and dots (0.2.0). A nightly build names its package version (0.1.1~nightly.20261031.5.gabc1234
# for RPM and Debian, 0.1.1nightly20261031.5 for Arch; `scripts/pkgrepo version` prints them), which also carries
# letters and a tilde. The archive and its top directory use the same string as the package, so the spec and the
# PKGBUILD find them.
case "$version" in
    *[!0-9A-Za-z.~]* | [!0-9]* | *.)
        echo "version must be digits and dots, optionally followed by a pre-release such as ~nightly.20261031.5.gabc1234" >&2
        exit 2
        ;;
esac

worktree="${BINGUX_ARCHIVE_WORKTREE:-0}"
if [[ "$worktree" != 1 && -n "$(git -C "$root" status --porcelain)" ]]; then
    echo "working tree has changes; commit the release source first" >&2
    echo "(BINGUX_ARCHIVE_WORKTREE=1 archives the working tree for a local packaging check)" >&2
    exit 1
fi
# Every pinned submodule (Quickshell for bingux-shell)
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

epoch="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"
archive_root="$(mktemp -d)"
trap 'rm -rf "$archive_root"' EXIT
archive_prefix="bingux-${version}"
mkdir -p "$archive_root/$archive_prefix"

if [[ "$worktree" == 1 ]]; then
    # Tracked and untracked files that exist now; a file deleted in the working
    # tree is still listed by the index, so skip what is gone. Submodules are
    # added below at their pinned revision.
    exclude=()
    for path in "${submodules[@]}"; do
        exclude+=(":!$path")
    done
    (
        cd "$root"
        git ls-files -z --cached --others --exclude-standard --deduplicate -- . "${exclude[@]}" |
            while IFS= read -r -d '' file; do
                [[ -e "$file" || -L "$file" ]] && printf '%s\0' "$file"
            done |
            tar --null --no-recursion -T - -cf -
    ) | tar -xf - -C "$archive_root/$archive_prefix"
else
    git -C "$root" archive --format=tar --prefix="${archive_prefix}/" HEAD |
        tar -xf - -C "$archive_root"
fi
for path in "${submodules[@]}"; do
    mkdir -p "$archive_root/$archive_prefix/$path"
    git -C "$root/$path" archive --format=tar HEAD |
        tar -xf - -C "$archive_root/$archive_prefix/$path"
done

# OUTPUT is relative to the checkout unless it is an absolute path.
[[ "$output" == /* ]] || output="$root/$output"
mkdir -p "$(dirname "$output")"
tar -C "$archive_root" --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${epoch}" \
    -cf - "$archive_prefix" | xz -T0 -9 >"$output"
echo "$output"
