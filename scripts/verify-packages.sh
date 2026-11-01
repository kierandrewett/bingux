#!/usr/bin/env bash
# Install the packages that scripts/build-packages.sh wrote in a clean container of their distribution
# (Fedora, Debian or Arch), check them, and remove them again.
#
#   scripts/verify-packages.sh PACKAGES_DIR
#
# The container has no build dependencies, so a dependency that pkgsmith/*.star forgot is a failure
# here. The steps:
#
#   1. Install the packages that bingux replaces (stand-ins from tests/pkgsmith/stubs.star), the way a
#      machine with an older release has them.
#   2. Make `gnoblin` and `xdg-desktop-portal-gnoblin` available: stand-ins by default, or the packages
#      of repo.gnoblin.org with GNOBLIN_SOURCE=repository.
#   3. Install bingux and the applications. The old packages must be gone afterwards.
#   4. Check that every program finds its shared libraries, that the files are as packaged, that the
#      scriptlets created the accounts, and that the recommended names exist in the distribution.
#   5. Remove the packages and check that nothing of them is left.
#
# Environment: PKGSMITH (the program that writes the stand-ins), VERSION, ARCH (amd64 or arm64),
# CHANNEL (stable or nightly: the Gnoblin channel when GNOBLIN_SOURCE=repository), GNOBLIN_SOURCE.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
packages="$(realpath "${1:?usage: scripts/verify-packages.sh PACKAGES_DIR}")"
PKGSMITH="${PKGSMITH:-pkgsmith}"
GNOBLIN_SOURCE="${GNOBLIN_SOURCE:-stub}"
CHANNEL="${CHANNEL:-nightly}"
ARCH="${ARCH:-$(case "$(uname -m)" in x86_64) echo amd64 ;; aarch64) echo arm64 ;; *) uname -m ;; esac)}"
ID="$(. /etc/os-release && echo "$ID")"
ALL="bingux bingux-settings bingux-files bingux-taxonomy"
OLD="bingux-config bingux-audio-effects bingux-preview-host"

stubs="$(mktemp -d)"
log() { printf '[verify] %s\n' "$*"; }
fail() {
    printf '[verify] FAILED: %s\n' "$*" >&2
    exit 1
}

# Write the stand-in packages of one kind in one format.
stub() {
    "$PKGSMITH" build -q -f "$root/tests/pkgsmith/stubs.star" --prefix "$root/tests/pkgsmith" \
        -F "$2" -o "$stubs/$1" --build-arg "KIND=$1" --build-arg "ARCH=$ARCH"
}

case "$ID" in
    fedora) family=rpm ;;
    debian) family=deb ;;
    arch) family=archlinux ;;
    *) fail "unsupported distribution $ID" ;;
esac

gnoblin_repository() {
    case "$family" in
        rpm)
            curl -fsSL -o "/etc/yum.repos.d/gnoblin-$CHANNEL.repo" "https://repo.gnoblin.org/gnoblin/$CHANNEL/fedora/gnoblin-$CHANNEL.repo"
            ;;
        archlinux)
            curl -fsSL -o /tmp/gnoblin.key https://repo.gnoblin.org/gpg.key
            pacman-key --init
            pacman-key --add /tmp/gnoblin.key
            pacman-key --lsign-key 91BAA3790941B9B3BBA63BC4F1B8D899DF3D786E
            printf '\n' >>/etc/pacman.conf
            curl -fsSL "https://repo.gnoblin.org/gnoblin/$CHANNEL/arch/gnoblin$([ "$CHANNEL" = stable ] || echo -nightly).conf" >>/etc/pacman.conf
            ;;
        deb)
            fail "repo.gnoblin.org has no APT repository yet; use GNOBLIN_SOURCE=stub"
            ;;
    esac
}

# ---- install -----------------------------------------------------------------------------------------------------

case "$family" in
    rpm)
        stub legacy rpm
        dnf -y install "$stubs"/legacy/*.rpm
        if [ "$GNOBLIN_SOURCE" = repository ]; then
            gnoblin_repository
            gnoblin_packages=(gnoblin xdg-desktop-portal-gnoblin)
        else
            stub gnoblin rpm
            gnoblin_packages=("$stubs"/gnoblin/*.rpm)
        fi
        mapfile -t files < <(find "$packages" -name '*.rpm' | sort)
        log "installing ${#files[@]} packages"
        dnf -y install --setopt=install_weak_deps=False "${gnoblin_packages[@]}" "${files[@]}"
        for old in $OLD; do
            ! rpm -q --quiet "$old" || fail "$old is still installed after the upgrade"
        done
        for package in $ALL; do rpm -q "$package"; done
        rpm -V $ALL || fail "rpm -V found a difference"
        # Every recommended name must exist in the distribution.
        for file in "${files[@]}"; do
            rpm -qp --recommends "$file" | sed 's/ .*//' | sort -u
        done | sort -u | grep -v '^bingux' | while read -r name; do
            dnf -q repoquery --whatprovides "$name" 2>/dev/null | grep -q . || fail "recommended $name does not exist"
        done
        ;;
    deb)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends ca-certificates curl
        stub legacy deb
        apt-get install -y -qq "$stubs"/legacy/*.deb
        [ "$GNOBLIN_SOURCE" = repository ] && gnoblin_repository
        stub gnoblin deb
        mapfile -t files < <(find "$packages" -name '*.deb' | sort)
        log "installing ${#files[@]} packages"
        apt-get install -y -qq --no-install-recommends "$stubs"/gnoblin/*.deb "${files[@]}"
        for old in $OLD; do
            ! dpkg -s "$old" 2>/dev/null | grep -q '^Status: install ok installed' || fail "$old is still installed after the upgrade"
        done
        for package in $ALL; do dpkg -s "$package" | grep -q '^Status: install ok installed'; done
        dpkg -V $ALL || fail "dpkg -V found a difference"
        for file in "${files[@]}"; do
            dpkg-deb -f "$file" Recommends | tr ',' '\n' | sed 's/^ *//; s/ .*//'
        done | sort -u | grep -v '^bingux\|^$' | while read -r name; do
            apt-cache show "$name" >/dev/null 2>&1 || fail "recommended $name does not exist"
        done
        ;;
    archlinux)
        pacman -Syu --noconfirm --needed curl >/dev/null
        stub legacy archlinux
        pacman -U --noconfirm "$stubs"/legacy/*.pkg.tar.zst
        if [ "$GNOBLIN_SOURCE" = repository ]; then
            gnoblin_repository
            pacman -Sy --noconfirm gnoblin xdg-desktop-portal-gnoblin
        else
            stub gnoblin archlinux
            pacman -U --noconfirm "$stubs"/gnoblin/*.pkg.tar.zst
        fi
        mapfile -t files < <(find "$packages" -name '*.pkg.tar.zst' | sort)
        log "installing ${#files[@]} packages"
        # --ask 4 answers yes to the question that removes the packages the new one conflicts with.
        pacman -U --noconfirm --ask 4 "${files[@]}"
        for old in $OLD bsm; do
            ! pacman -Q "$old" >/dev/null 2>&1 || fail "$old is still installed after the upgrade"
        done
        for package in $ALL; do pacman -Q "$package"; done
        pacman -Qkk $ALL | tee /tmp/pacman-qkk.txt
        ! grep -q 'warning\|error' /tmp/pacman-qkk.txt || fail "pacman -Qkk found a difference"
        for file in "${files[@]}"; do
            pacman -Qip "$file" | sed -n 's/^Optional Deps *: *//p;s/^ *\([a-z][^ :]*\):.*/\1/p'
        done | sed 's/:.*//' | sort -u | grep -v '^bingux\|^None$\|^$' | while read -r name; do
            pacman -Si "$name" >/dev/null 2>&1 || fail "optional dependency $name does not exist"
        done
        ;;
esac

# ---- check -------------------------------------------------------------------------------------------------------

log "libraries"
"$root/scripts/check-package-libs.sh" $ALL

log "programs"
# Each program starts far enough to print its usage (the exit status of a usage message differs by
# program); the shell prints its version without a display.
QT_QPA_PLATFORM=offscreen bingux-shell --version
for program in bingux-config bsm bsmctl bingux-effects bingux-helper bingux-session bingux-searchd; do
    output="$("$program" --help 2>&1 || true)"
    [ -n "$output" ] || fail "$program printed nothing"
    ! grep -q 'error while loading shared libraries' <<<"$output" || fail "$program: $output"
done
test -x /usr/bin/bsm && test -x /usr/libexec/bsm/bsm-helper
test -f /usr/lib/systemd/user/bingux.target && test -f /usr/lib/systemd/system/bsm.service
test -f /usr/lib/systemd/user-preset/90-bingux.preset
# The scriptlets create the accounts of bsm and the directories bsm and the greeter use.
getent passwd bsm >/dev/null || fail "the account bsm was not created"
getent passwd bingux-greeter >/dev/null || fail "the account bingux-greeter was not created"
test -d /var/lib/bsm
# Installing must not change how the machine logs in.
test ! -e /etc/systemd/system/display-manager.service || fail "installing bingux changed the display manager"
# The shell's runtime files name no build path.
! grep -rIl -e '/home/' -e '/__w/' -e '/build/' /usr/bin/bingux* /usr/libexec/bingux /usr/lib*/systemd/user/bingux* 2>/dev/null | grep -q . || fail "an installed file names a build path"

# ---- remove ------------------------------------------------------------------------------------------------------

log "removal"
case "$family" in
    # Only these packages: the dependencies they pulled in stay (a base image protects some of them).
    rpm) dnf -y remove --setopt=clean_requirements_on_remove=False $ALL ;;
    deb) apt-get remove -y -qq $ALL ;;
    archlinux) pacman -R --noconfirm $ALL ;;
esac
for path in /usr/bin/bingux-shell /usr/bin/bingux-settings /usr/bin/bsm /usr/lib/systemd/user/bingux.target; do
    test ! -e "$path" || fail "$path is left after the removal"
done
log "ok"
