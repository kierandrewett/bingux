#!/usr/bin/env bash
# Write the packages of Bingux from a prefix with pkgsmith. pkgsmith builds nothing: the prefix comes from
# scripts/build-prefix.sh. docs/packaging.md describes the whole path.
#
#   scripts/build-packages.sh
#   PREFIX_DIR=/tmp/p PACKAGE_DIR=/tmp/out VERSION=0.1.1-nightly.20261031.142.gabc1234 FORMATS=rpm scripts/build-packages.sh
#
# Environment:
#   PREFIX_DIR    the prefix                       (default: ./prefix)
#   PACKAGE_DIR   where the packages are written   (default: ./dist)
#   VERSION       the version, as pkgsmith spells it: 0.1.0 or 0.1.1-nightly.20261031.142.gabc1234
#                 (default: VERSION ?= of the Makefile)
#   FORMATS       comma separated formats of pkgsmith: rpm,deb,archlinux,tar (default: all four)
#   PKGSMITH      the pkgsmith program             (default: pkgsmith on PATH)
#
# scripts/build-prefix.sh leaves PREFIX_DIR.env beside the prefix with the facts of the machine that built
# it (the Qt minor version, the architecture and the distribution). They go to the scripts as build
# arguments, so a prefix can be packaged on another machine.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

PREFIX_DIR="$(realpath -m "${PREFIX_DIR:-$root/prefix}")"
PACKAGE_DIR="$(realpath -m "${PACKAGE_DIR:-$root/dist}")"
VERSION="${VERSION:-$(sed -n 's/^VERSION ?= //p' Makefile)}"
PKGSMITH="${PKGSMITH:-pkgsmith}"

if [ ! -d "$PREFIX_DIR" ]; then
    echo "build-packages.sh: $PREFIX_DIR does not exist; run scripts/build-prefix.sh first" >&2
    exit 1
fi
if [ ! -r "$PREFIX_DIR.env" ]; then
    echo "build-packages.sh: $PREFIX_DIR.env is missing; scripts/build-prefix.sh writes it" >&2
    exit 1
fi
# shellcheck source=/dev/null
. "$PREFIX_DIR.env"

args=(--prefix "$PREFIX_DIR" --build-arg "VERSION=$VERSION" --build-arg "ARCH=$PREFIX_ARCH" --build-arg "QT_MINOR=$PREFIX_QT_MINOR"
    --build-arg "RPM_DIST=$PREFIX_RPM_DIST" --build-arg "DISTRO=$PREFIX_DISTRO")
if [ -n "${FORMATS:-}" ]; then
    args+=(-F "$FORMATS")
fi

mkdir -p "$PACKAGE_DIR"
# One script for each family: the bingux package, then the applications that depend on it.
for script in bingux apps; do
    "$PKGSMITH" build "${args[@]}" -f "pkgsmith/$script.star" -o "$PACKAGE_DIR"
done
echo "Packages in $PACKAGE_DIR:"
ls -1 "$PACKAGE_DIR"
