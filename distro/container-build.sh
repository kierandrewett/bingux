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
# mktemp creates it 0700 for root; the builder account must traverse it.
chmod 755 "$STAGE"
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
    allow_rootless_chroot
    register_repo
}

# arch-chroot (used by pacstrap -c and mkarchiso) mounts a fresh devtmpfs in
# the chroot. A rootless container's user namespace cannot mount devtmpfs, so
# the chroot gets a recursive bind of the container's /dev instead, which is
# what arch-chroot's own unshare mode does.
allow_rootless_chroot() {
    local probe
    probe="$(mktemp -d)"
    if mount udev "$probe" -t devtmpfs -o mode=0755,nosuid 2>/dev/null; then
        umount "$probe"
        rmdir "$probe"
        return 0
    fi
    rmdir "$probe"
    log "rootless container: chroots bind /dev instead of mounting devtmpfs"
    # pacstrap carries its own copy of chroot_setup.
    local script
    for script in /usr/bin/arch-chroot /usr/bin/pacstrap; do
        sed -i 's|chroot_add_mount udev "$1/dev" -t devtmpfs -o mode=0755,nosuid|chroot_add_mount /dev "$1/dev" --rbind|' \
            "$script"
        grep -q 'chroot_add_mount /dev "$1/dev" --rbind' "$script"
    done
}

# makepkg refuses to run as root; build as `builder` with the sources copied
# next to the PKGBUILD. A local repository lets later packages depend on
# earlier ones (bingux depends on gnoblin).
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
        bingux | bsm)
            local archive
            archive="$(ls "$DISTRO"/sources/bingux-*.tar.xz | head -n 1)"
            [ -f "$archive" ] || {
                log "missing Bingux source archive in distro/sources"
                exit 1
            }
            cp "$archive" "$dir/"
            ;;
        bingux-installer)
            tar -C "$DISTRO" -cJf "$dir/bingux-installer-src.tar.xz" \
                --exclude=tests/vm --exclude=__pycache__ --exclude='*.egg-info' \
                --exclude=dist --exclude=.pytest_cache installer \
                -C "$WORK" shell/bingux scripts/pack-shell.py
            ;;
    esac
    chown -R builder:builder "$dir" "$REPO"
    # -f: a rerun with regenerated sources replaces the package of the same version.
    (cd "$dir" && sudo -u builder env MAKEFLAGS="-j$JOBS" PKGDEST="$REPO" makepkg -sf --noconfirm --skippgpcheck)
    register_repo
    release_output
}

# Container root is the host user under rootless podman; outputs owned by
# the builder account would be unwritable on the host.
release_output() {
    chown -R 0:0 "$OUT"
}

# Later packages resolve their dependencies from the repository built so far;
# a package:NAME run uses the repository of an earlier run. The database is
# rebuilt from the package files, so a run that was interrupted after makepkg
# but before repo-add still counts its package.
register_repo() {
    ls "$REPO"/*.pkg.tar.* >/dev/null 2>&1 || return 0
    log "registering the package repository"
    # A fresh database: repo-add cannot update one that carries SELinux
    # attributes from a host checkout.
    rm -f "$REPO"/bingux-os.db "$REPO"/bingux-os.db.tar.gz "$REPO"/bingux-os.files "$REPO"/bingux-os.files.tar.gz
    repo-add -q -R "$REPO/bingux-os.db.tar.gz" "$REPO"/*.pkg.tar.*
    if ! grep -q '^\[bingux-os\]' /etc/pacman.conf; then
        printf '\n[bingux-os]\nSigLevel = Optional TrustAll\nServer = file://%s\n' "$REPO" >>/etc/pacman.conf
    fi
    pacman -Sy >/dev/null
}

build_packages() {
    build_package gnoblin
    build_package bingux
    build_package bsm
    build_package python-blivet
    build_package bingux-installer
    build_package bingux-os
}

# archiso's releng profile supplies the boot loaders and the base
# configuration; the Bingux OS profile overlays the desktop on top.
build_iso() {
    local profile="$STAGE/profile"
    log "assembling the archiso profile"
    rm -rf "$profile"
    cp -r /usr/share/archiso/configs/releng "$profile"
    cp -a "$DISTRO/profile/." "$profile/"
    # bsm is opt-in: units and drop-ins that start it only with `bingux.bsm` on
    # the kernel command line and keep greetd off then (packaging/bsm/image).
    mkdir -p "$profile/airootfs/etc/systemd/system"
    cp -a "$WORK/packaging/bsm/image/." "$profile/airootfs/etc/systemd/system/"
    printf '\n[bingux-os]\nSigLevel = Optional TrustAll\nServer = file://%s\n' "$REPO" >>"$profile/pacman.conf"
    # Login screen, wallpaper and skel come from the bingux-os package, which
    # archinstall installs as well.
    # The repository travels on the image so the installer finds the Bingux OS
    # packages without the network (profile/airootfs/etc/pacman.d/bingux-os-mirrorlist).
    local repo_copy="$profile/airootfs/opt/bingux-os/repo"
    mkdir -p "$repo_copy"
    cp -aL "$REPO"/. "$repo_copy"/
    log "building the live image (this takes a while)"
    mkdir -p "$OUT"
    mkarchiso -v -w "$STAGE/iso-work" -o "$OUT" "$profile"
    release_output
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
    package:*)
        # One package against the repository built so far, for PKGBUILD work
        # without rebuilding Mutter.
        prepare_host
        build_package "${STEP#package:}"
        ;;
    *)
        echo "usage: container-build.sh [all|packages|iso|package:NAME]" >&2
        exit 2
        ;;
esac
