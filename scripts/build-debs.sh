#!/usr/bin/env bash
# Build the Debian packages from packaging/debian.
#
# usage: scripts/build-debs.sh [VERSION [TOPDIR]]
#
# VERSION   the package version without the Debian revision; defaults to 0.1.0 (the Makefile's VERSION). A nightly
#           build passes its Debian version, such as 0.1.1~nightly.20261031.5.gabc1234 (scripts/pkgrepo version).
#           The package version is VERSION-1.
# TOPDIR    defaults to /var/tmp/BinguxPkg; the packages land in TOPDIR/deb/out
#
# On Debian or Ubuntu (a CI container too) with dpkg-buildpackage and the build dependencies installed
# (scripts/debian-build-deps.sh installs them on Debian 13), this runs it directly. Anywhere else (Fedora has a dpkg that cannot build for Debian) it runs it in a debian:trixie container with podman or docker; the container needs the network to
# install the build dependencies, to fetch the Rust crates and to download the prebuilt Skia archive.
#
# Environment:
#   BINGUX_ARCHIVE_WORKTREE=1   archive the working tree instead of HEAD (a local packaging check of uncommitted
#                               work; never publish)
#   BINGUX_DEB_IMAGE            the container image (default docker.io/library/debian:trixie)
#   JOBS                        parallel jobs (default: the CPU count)
#   SOURCE_DATE_EPOCH           build time stamp (default: the commit time of HEAD)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="${1:-0.1.0}"
top="${2:-/var/tmp/BinguxPkg}"
work="$top/deb"
image="${BINGUX_DEB_IMAGE:-docker.io/library/debian:trixie}"
jobs="${JOBS:-$(nproc)}"

export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"

rm -rf "$work"
mkdir -p "$work/out"
archive="$work/bingux-${version}.tar.xz"
bash "$root/scripts/make-source-archive.sh" "$version" "$archive" >/dev/null
tar -C "$work" -xf "$archive"
src="$work/bingux-${version}"
cp -r "$root/packaging/debian" "$src/debian"
# The changelog names the version being built; dpkg-buildpackage takes the package version from it.
date="$(date -u -d "@$SOURCE_DATE_EPOCH" '+%a, %d %b %Y %H:%M:%S +0000')"
cat >"$src/debian/changelog" <<CHANGELOG
bingux (${version}-1) unstable; urgency=medium

  * Build of ${version}.

 -- Kieran Drewett <kieran@drewett.dev>  ${date}
CHANGELOG

if [[ -f /etc/debian_version ]] && command -v dpkg-buildpackage >/dev/null; then
    (cd "$src" && DEB_BUILD_OPTIONS="parallel=$jobs nocheck" dpkg-buildpackage -us -uc -b)
else
    if command -v podman >/dev/null; then
        engine=podman
    elif command -v docker >/dev/null; then
        engine=docker
    else
        echo "build-debs: dpkg-buildpackage, podman or docker is required" >&2
        exit 1
    fi
    "$engine" run --rm -v "$work:/work:Z" -e "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" -e "JOBS=$jobs" \
        -e "BINGUX_SRC=/work/bingux-${version}" "$image" bash -euc '
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends build-essential fakeroot ca-certificates curl git >/dev/null
        cd "$BINGUX_SRC"
        bash scripts/debian-build-deps.sh
        DEB_BUILD_OPTIONS="parallel=$JOBS nocheck" dpkg-buildpackage -us -uc -b
    '
fi
mv "$work"/*.deb "$work/out/"

echo
echo "Packages in $work/out:"
find "$work/out" -name '*.deb' | sort
