#!/usr/bin/env bash
# Build the Bingux OS packages and live image inside an Arch Linux container.
# distro/build.sh runs this; see distro/README.md for the inputs it expects.
#
# Inputs (mounted at /work):
#   /work/distro/sources/gnoblin-*-source.tar.xz   Gnoblin source bundle
#   /work/distro/sources/gnoblin-*.PKGBUILD        matching generated PKGBUILD
#   /work/distro/sources/bingux-*.tar.xz           Bingux source archive
# Outputs:
#   /work/distro/out/repo/      pacman repository with the built packages
#   /work/distro/out/*.iso      the live image
set -euo pipefail

WORK=/work
DISTRO="$WORK/distro"
OUT="$DISTRO/out"
REPO="$OUT/repo"
STAGE="$(mktemp -d /tmp/bingux-os.XXXXXX)"
JOBS="${JOBS:-$(nproc)}"
STEP="${1:-all}"

log() { printf '[distro] %s\n' "$*"; }

prepare_host() {
    log "updating the container and installing build tools"
    pacman -Syu --noconfirm --needed base-devel git archiso imagemagick sudo >/dev/null
    if ! id builder >/dev/null 2>&1; then
        useradd -m builder
        echo 'builder ALL=(ALL) NOPASSWD: ALL' >/etc/sudoers.d/builder
    fi
    mkdir -p "$REPO"
    chown -R builder:builder "$OUT"
}

# makepkg refuses to run as root; build as `builder` with the sources copied
# next to the PKGBUILD. A local repository lets later packages depend on
# earlier ones (bingux depends on quickshell and gnoblin).
build_package() {
    local name="$1"
    local dir="$STAGE/$name"
    log "building $name"
    rm -rf "$dir"
    cp -a "$DISTRO/packages/$name" "$dir"
    case "$name" in
        gnoblin)
            local bundle pkgbuild
            bundle="$(ls "$DISTRO"/sources/gnoblin-*-source.tar.xz | head -n 1)"
            pkgbuild="$(ls "$DISTRO"/sources/gnoblin-*.PKGBUILD | head -n 1)"
            [ -f "$bundle" ] && [ -f "$pkgbuild" ] || {
                log "missing Gnoblin source bundle or PKGBUILD in distro/sources"
                exit 1
            }
            cp "$bundle" "$dir/"
            # The release PKGBUILD downloads from GitHub; build the local bundle instead.
            sed -e "s|^source=(.*|source=(\"$(basename "$bundle")\")|" \
                -e "s|^sha256sums=(.*|sha256sums=('SKIP')|" "$pkgbuild" >"$dir/PKGBUILD"
            ;;
        bingux)
            local archive
            archive="$(ls "$DISTRO"/sources/bingux-*.tar.xz | head -n 1)"
            [ -f "$archive" ] || {
                log "missing Bingux source archive in distro/sources"
                exit 1
            }
            cp "$archive" "$dir/"
            ;;
        quickshell)
            cp "$WORK"/packages/bingux-quickshell/*.patch "$dir/"
            ;;
    esac
    chown -R builder:builder "$dir"
    (cd "$dir" && sudo -u builder env MAKEFLAGS="-j$JOBS" PKGDEST="$REPO" makepkg -s --noconfirm --skippgpcheck)
    repo-add -R "$REPO/bingux-os.db.tar.gz" "$REPO"/*.pkg.tar.* >/dev/null
    # Later packages resolve their dependencies from this repository.
    if ! grep -q '^\[bingux-os\]' /etc/pacman.conf; then
        printf '\n[bingux-os]\nSigLevel = Optional TrustAll\nServer = file://%s\n' "$REPO" >>/etc/pacman.conf
    fi
    pacman -Sy >/dev/null
}

build_packages() {
    build_package quickshell
    build_package gnoblin
    build_package bingux
}

# archiso's releng profile supplies the boot loaders and the base
# configuration; the Bingux OS profile overlays the desktop on top.
build_iso() {
    local profile="$STAGE/profile"
    log "assembling the archiso profile"
    rm -rf "$profile"
    cp -r /usr/share/archiso/configs/releng "$profile"
    cp -a "$DISTRO/profile/." "$profile/"
    printf '\n[bingux-os]\nSigLevel = Optional TrustAll\nServer = file://%s\n' "$REPO" >>"$profile/pacman.conf"
    # The default wallpaper is generated, not checked in: a deep two-tone
    # gradient the greeter and the first session share.
    local backgrounds="$profile/airootfs/usr/share/backgrounds/bingux-os"
    mkdir -p "$backgrounds"
    magick -size 2560x1600 gradient:'#2b3a67-#101523' -attenuate 0.15 +noise Gaussian -quality 92 "$backgrounds/default.jpg"
    # Users created by archinstall and the live account get the Gnoblin
    # init.lua from /etc/skel; root needs nothing.
    log "building the live image (this takes a while)"
    mkdir -p "$OUT"
    mkarchiso -v -w "$STAGE/iso-work" -o "$OUT" "$profile"
    chown -R builder:builder "$OUT"
    log "done: $(ls "$OUT"/*.iso)"
}

case "$STEP" in
    all)
        prepare_host
        build_packages
        build_iso
        ;;
    packages)
        prepare_host
        build_packages
        ;;
    iso)
        prepare_host
        build_iso
        ;;
    *)
        echo "usage: container-build.sh [all|packages|iso]" >&2
        exit 2
        ;;
esac
