#!/usr/bin/env bash
# Build all of Bingux into a prefix: a folder laid out like the installed system.
# pkgsmith (scripts/build-packages.sh) reads that folder and writes the packages;
# it builds nothing itself. docs/packaging.md describes the whole path.
#
#   scripts/build-prefix.sh                 build, then install into ./prefix
#   PREFIX_DIR=/tmp/p JOBS=8 scripts/build-prefix.sh
#
# The shell links Qt's private API and the C++ runtime of the machine it is built on, so build the
# prefix on the distribution you package for (Fedora 43, Debian 13, Arch). The Rust programs link
# the C library of the same machine. Start from an empty prefix: the script removes the old one.
#
# Environment:
#   PREFIX_DIR    the prefix to write            (default: ./prefix)
#   BUILD_DIR     the build tree                 (default: ./build)
#   CARGO_TARGET  Cargo's target directory       (default: BUILD_DIR/cargo)
#   JOBS          parallel jobs                  (default: nproc)
#   VERSION       the version being built; a pre-release suffix is cut off for the shell (default: Makefile)
#   LIBDIR_NAME   lib or lib64: where the shell archive and the QML plugins go
#                 (default: lib64 on Fedora, lib elsewhere)
#   SKIP_BUILD=1  install what BUILD_DIR and CARGO_TARGET already hold
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

PREFIX_DIR="$(realpath -m "${PREFIX_DIR:-$root/prefix}")"
BUILD_DIR="$(realpath -m "${BUILD_DIR:-$root/build}")"
CARGO_TARGET="$(realpath -m "${CARGO_TARGET:-$BUILD_DIR/cargo}")"
JOBS="${JOBS:-$(nproc)}"
VERSION="${VERSION:-$(sed -n 's/^VERSION ?= //p' Makefile)}"
base_version="${VERSION%%-*}"
if [ -z "${LIBDIR_NAME:-}" ]; then
    LIBDIR_NAME=lib
    if [ -r /etc/os-release ] && grep -qE '^ID=("?)fedora' /etc/os-release; then
        LIBDIR_NAME=lib64
    fi
fi

log() { printf '[prefix] %s\n' "$*"; }

if [ "$PREFIX_DIR" = / ] || [ "$PREFIX_DIR" = "$root" ]; then
    echo "build-prefix.sh: refusing to use $PREFIX_DIR as the prefix" >&2
    exit 1
fi

# The same Rust flags the RPM used: line-table debug info keeps the statically linked Freya and
# Skia programs a usable size until they are stripped below. Hardening flags reach the C helpers
# and the Qt plugins through the Makefile, as distribution builds pass them.
export RUSTFLAGS="${RUSTFLAGS:--Copt-level=3 -Cdebuginfo=line-tables-only -Cstrip=none -Cforce-frame-pointers=yes --cap-lints=warn}"
hardening="-O2 -g -fstack-protector-strong -fstack-clash-protection -fPIE -D_FORTIFY_SOURCE=2"
case "$(uname -m)" in
    x86_64) hardening="$hardening -fcf-protection" ;;
    aarch64) hardening="$hardening -mbranch-protection=standard" ;;
esac
export CFLAGS="${CFLAGS:-$hardening}"
export CXXFLAGS="${CXXFLAGS:-$hardening}"
export LDFLAGS="${LDFLAGS:--pie -Wl,-z,relro,-z,now}"

make_args=(BUILD_DIR="$BUILD_DIR" CARGO_TARGET="$CARGO_TARGET" JOBS="$JOBS" LIBDIR_NAME="$LIBDIR_NAME" VERSION="$base_version")

if [ "${SKIP_BUILD:-0}" != 1 ]; then
    log "building everything with $JOBS jobs (shell, native plugins, daemons, apps)"
    make "${make_args[@]}" all
fi

log "installing into $PREFIX_DIR"
rm -rf "$PREFIX_DIR"
mkdir -p "$PREFIX_DIR"
# The installer writes absolute paths of the tools it resolves into the search configuration, and
# of the interpreter into the wrappers. Run it with a system PATH so they name /usr/bin, never a
# toolchain or home folder of the build machine.
system_path=/usr/sbin:/usr/bin:/sbin:/bin
PATH="$system_path" make "${make_args[@]}" install PREFIX=/usr DESTDIR="$PREFIX_DIR" QMLDIR="/usr/$LIBDIR_NAME/bingux/qml"

# The scripts name /usr/bin/env; every target system has /usr/bin/python3, and packaging policies
# of Fedora and Debian want the interpreter itself.
if [ -d "$PREFIX_DIR/usr/libexec/bingux" ]; then
    grep -rIl '^#!/usr/bin/env python3' "$PREFIX_DIR/usr/libexec/bingux" |
        xargs -r sed -i '1s|^#!/usr/bin/env python3|#!/usr/bin/python3|'
fi

# The PAM services of the lock screen. They are configuration files of the system, so they are not
# part of `make install`, which also installs below /usr/local.
install -Dm644 data/pam/bingux-lock "$PREFIX_DIR/etc/pam.d/bingux-lock"
install -Dm644 data/pam/bingux-lock-fingerprint "$PREFIX_DIR/etc/pam.d/bingux-lock-fingerprint"

# Documents and licences, for the package that ships each program.
doc() { install -Dm644 "docs/$2" "$PREFIX_DIR/usr/share/doc/$1/$2"; }
install -Dm644 README.md "$PREFIX_DIR/usr/share/doc/bingux/README.md"
for page in extensions.md locking.md login-screen.md configuration.md audio-effects.md; do doc bingux "$page"; done
doc bingux-settings settings.md
doc bingux-files files.md
doc bingux-taxonomy bingux-ui.md
for package in bingux bingux-settings bingux-files bingux-taxonomy; do
    install -Dm644 COPYING "$PREFIX_DIR/usr/share/licenses/$package/COPYING"
done
# Quickshell is inside the bingux-shell binary; its licence is LGPL.
install -Dm644 shell/runtime/quickshell/LICENSE "$PREFIX_DIR/usr/share/licenses/bingux/LICENSE.quickshell"
python3 scripts/cargo-licenses.py --out "$BUILD_DIR/licenses" --prefix "$PREFIX_DIR"

# Man pages are compressed by rpmbuild, dh_compress and makepkg; pkgsmith packs files as they are.
if [ -d "$PREFIX_DIR/usr/share/man" ]; then
    find "$PREFIX_DIR/usr/share/man" -type f -name '*.[0-9]' -exec gzip -9n {} +
fi

# Strip the programs and the plugins: the packages carry no debug information.
if command -v strip >/dev/null; then
    find "$PREFIX_DIR" -type f -size +0 | while read -r file; do
        if [ "$(head -c4 "$file" | od -An -c | tr -d ' ')" = '177ELF' ]; then
            strip --strip-unneeded "$file"
        fi
    done
fi

# A prefix that names the build machine would install a broken desktop. Documents may mention any path.
leaks="$(grep -rIl --exclude-dir=doc -e "$PREFIX_DIR" -e "$root" -e "${HOME:-/nonexistent-home}/" "$PREFIX_DIR" || true)"
if [ -n "$leaks" ]; then
    echo "build-prefix.sh: these files name the build machine's paths:" >&2
    echo "$leaks" >&2
    exit 1
fi

# What scripts/build-packages.sh needs to know about the machine that built the prefix. It is a file
# beside the prefix, not in it, so no package carries it.
os_id="$(. /etc/os-release && echo "$ID")"
os_version="$(. /etc/os-release && echo "${VERSION_ID:-}")"
case "$(uname -m)" in
    x86_64) arch=amd64 ;;
    aarch64) arch=arm64 ;;
    *) arch="$(uname -m)" ;;
esac
rpm_dist=
if [ "$os_id" = fedora ]; then
    rpm_dist=".fc$os_version"
fi
# Arch is a rolling release: its VERSION_ID is a build date, not a release.
distro_name="$os_id$os_version"
if [ "$os_id" = arch ]; then
    distro_name=arch
fi
qt_minor="$(pkg-config --modversion Qt6Core | cut -d. -f1,2)"
cat >"$PREFIX_DIR.env" <<ENV
PREFIX_ARCH=$arch
PREFIX_QT_MINOR=$qt_minor
PREFIX_RPM_DIST=$rpm_dist
PREFIX_DISTRO=$distro_name
ENV
log "done: $(find "$PREFIX_DIR" -type f | wc -l) files in $PREFIX_DIR ($(cat "$PREFIX_DIR.env" | tr '\n' ' '))"
