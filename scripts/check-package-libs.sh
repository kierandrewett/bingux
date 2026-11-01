#!/usr/bin/env bash
# Check that every program of installed packages finds its shared libraries. pkgsmith does not look
# inside the programs, so the dependency lists of pkgsmith/*.star name the libraries by hand, and this
# is the check that they are complete: install the packages in a clean container of the distribution,
# then run
#
#   scripts/check-package-libs.sh bingux bingux-settings bingux-files bingux-taxonomy
#
# It lists the ELF files of each package with the package manager of the system (rpm, dpkg or
# pacman), runs ldd on them and fails when a library is "not found". Libraries a program opens with
# dlopen are not visible to ldd; the lists in pkgsmith/apps.star name those.
set -euo pipefail

if [ $# -eq 0 ]; then
    echo "usage: scripts/check-package-libs.sh PACKAGE..." >&2
    exit 2
fi

list_files() {
    if command -v rpm >/dev/null; then
        rpm -ql "$1"
    elif command -v dpkg >/dev/null; then
        dpkg -L "$1"
    else
        pacman -Qlq "$1"
    fi
}

status=0
checked=0
for package in "$@"; do
    while read -r file; do
        [ -f "$file" ] && [ ! -L "$file" ] || continue
        [ "$(head -c4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ] || continue
        checked=$((checked + 1))
        missing="$(ldd "$file" 2>&1 | grep -E 'not found' || true)"
        if [ -n "$missing" ]; then
            echo "$package: $file"
            echo "$missing" | sed 's/^/    /'
            status=1
        fi
    done < <(list_files "$package")
done
echo "checked $checked ELF files"
exit "$status"
