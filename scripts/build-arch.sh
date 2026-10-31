#!/usr/bin/env bash
# Build the Arch packages from packaging/arch/PKGBUILD.
#
# usage: scripts/build-arch.sh [VERSION [TOPDIR]]
#
# VERSION   defaults to 0.1.0 (the Makefile's VERSION); a nightly build passes its Arch version, such as
#           0.1.1nightly20261031.5 (scripts/pkgrepo version)
# TOPDIR    defaults to /var/tmp/BinguxPkg; the packages land in TOPDIR/arch/out
#
# On Arch (makepkg installed) this runs makepkg directly. Anywhere else it runs
# makepkg in an archlinux container with podman or docker, as Bingux OS's own
# distro/build.sh does; the container needs the network to install the build
# dependencies.
#
# Environment:
#   BINGUX_ARCHIVE_WORKTREE=1   archive the working tree instead of HEAD (a local
#                               packaging check of uncommitted work; never publish)
#   JOBS                        parallel jobs (default: the CPU count)
#   SOURCE_DATE_EPOCH           build time stamp (default: the commit time of HEAD)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="${1:-0.1.0}"
top="${2:-/var/tmp/BinguxPkg}"
work="$top/arch"

export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"

rm -rf "$work"
mkdir -p "$work/src" "$work/out"
archive="$work/src/bingux-${version}.tar.xz"
bash "$root/scripts/make-source-archive.sh" "$version" "$archive" >/dev/null
sum="$(sha256sum "$archive" | cut -d' ' -f1)"
# The checked-in PKGBUILD carries SKIP; the build copy carries the real checksum.
sed -e "s/^pkgver=.*/pkgver=${version}/" -e "s/^sha256sums=.*/sha256sums=('${sum}')/" \
    "$root/packaging/arch/PKGBUILD" >"$work/src/PKGBUILD"
cp "$root/packaging/arch/bingux.install" "$work/src/"

if command -v makepkg >/dev/null && [[ "$(id -u)" != 0 ]]; then
    (cd "$work/src" && PKGDEST="$work/out" MAKEFLAGS="-j${JOBS:-$(nproc)}" makepkg -sf --noconfirm)
else
    if command -v podman >/dev/null; then
        engine=podman
    elif command -v docker >/dev/null; then
        engine=docker
    else
        echo "build-arch: makepkg, podman or docker is required" >&2
        exit 1
    fi
    # makepkg refuses to run as root, so the container builds as a `builder` account.
    "$engine" run --rm -v "$work:/work:Z" -e "SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH" \
        -e "JOBS=${JOBS:-$(nproc)}" docker.io/library/archlinux:latest bash -euc '
        pacman -Syu --noconfirm --needed base-devel sudo >/dev/null
        id builder >/dev/null 2>&1 || useradd -m builder
        echo "builder ALL=(ALL) NOPASSWD: ALL" >/etc/sudoers.d/builder
        chown -R builder:builder /work
        cd /work/src
        sudo -u builder env SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH" PKGDEST=/work/out MAKEFLAGS="-j$JOBS" \
            makepkg -sf --noconfirm
    '
fi

echo
echo "Packages in $work/out:"
find "$work/out" -name '*.pkg.tar.*' | sort
