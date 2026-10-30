#!/usr/bin/env bash
# Build Bingux OS from any Linux host with podman or docker.
#
# usage: distro/build.sh [--gnoblin PATH] [--step all|packages|iso|package:NAME]
#
# 1. Produces the source inputs under distro/sources/:
#    - the Bingux source archive from this checkout (committed tree)
#    - the Gnoblin source bundle and PKGBUILD from the Gnoblin checkout
#      (`--gnoblin`, default ../gnoblin; must be a committed tree)
# 2. Runs distro/container-build.sh in an archlinux container, which builds
#    the packages into distro/out/repo and the live image into distro/out.
#
# mkarchiso needs loop devices and squashfs; the container runs privileged.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GNOBLIN="${GNOBLIN_ROOT:-$ROOT/../gnoblin}"
STEP=all
while [ $# -gt 0 ]; do
    case "$1" in
        --gnoblin)
            GNOBLIN="$2"
            shift 2
            ;;
        --step)
            STEP="$2"
            shift 2
            ;;
        *)
            echo "usage: distro/build.sh [--gnoblin PATH] [--step all|packages|iso|package:NAME]" >&2
            exit 2
            ;;
    esac
done
GNOBLIN="$(cd "$GNOBLIN" && pwd)"

if command -v podman >/dev/null 2>&1; then
    ENGINE=podman
elif command -v docker >/dev/null 2>&1; then
    ENGINE=docker
else
    echo "distro/build.sh: podman or docker is required" >&2
    exit 1
fi

SOURCES="$ROOT/distro/sources"
mkdir -p "$SOURCES" "$ROOT/distro/out"

# Each source input is regenerated only for the steps that build from it, so a
# quick `--step package:bingux-os` needs neither a clean tree nor Gnoblin.
case "$STEP" in
    all | packages) need_bingux=1 need_gnoblin=1 ;;
    package:bingux | package:bsm) need_bingux=1 need_gnoblin=0 ;;
    package:gnoblin) need_bingux=0 need_gnoblin=1 ;;
    *) need_bingux=0 need_gnoblin=0 ;;
esac

if [ "$need_bingux" = 1 ]; then
    version="$(sed -n 's/^VERSION ?= //p' "$ROOT/Makefile")"
    echo "[distro] Bingux source archive $version"
    rm -f "$SOURCES"/bingux-*.tar.xz
    (cd "$ROOT" && bash scripts/make-source-archive.sh "$version" "distro/sources/bingux-$version.tar.xz")
fi

if [ "$need_gnoblin" = 1 ]; then
    echo "[distro] Gnoblin source bundle from $GNOBLIN"
    rm -f "$SOURCES"/gnoblin-*
    release="$(mktemp -d)"
    rmdir "$release"
    (cd "$GNOBLIN" && bash scripts/build-release-assets.sh "$release" "" --source-only)
    cp "$release"/gnoblin-*-source.tar.xz "$release"/gnoblin-*.PKGBUILD "$SOURCES/"
    rm -rf "$release"
fi

# The build stages the live root and the compile trees under /tmp, several GB.
# On a nearly full disk that is the first thing to run out, so keep it in
# memory: BINGUX_BUILD_TMPFS sets the size (default 20g); 0 uses the
# container's own disk-backed /tmp.
tmpfs_size="${BINGUX_BUILD_TMPFS:-20g}"
tmpfs_args=()
if [ "$tmpfs_size" != 0 ]; then
    tmpfs_args=(--tmpfs "/tmp:rw,exec,mode=1777,size=$tmpfs_size")
fi

echo "[distro] building in an $ENGINE archlinux container"
exec "$ENGINE" run --rm --privileged \
    "${tmpfs_args[@]}" \
    -v "$ROOT:/work:Z" \
    -e JOBS="${JOBS:-$(nproc)}" \
    docker.io/library/archlinux:latest \
    bash /work/distro/container-build.sh "$STEP"
