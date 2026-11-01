#!/usr/bin/env bash
# Install bsm, the Bingux Session Manager: its programs and the system files
# around them. `make install-bsm` runs this after `make bsm`.
#
#   scripts/install-bsm.sh --prefix /usr --destdir DIR --cargo-dir TARGET
#
# --prefix must be /usr. The units, the sysusers file and the polkit rules name
# /usr/bin, /usr/libexec/bsm and /usr/lib/systemd, and the files below /etc and
# /usr/share/dbus-1 are read from fixed places. Installing the files enables
# nothing: bsm becomes the display manager only when an administrator runs
# `bsm-set-display-manager enable` (docs/bsm/packaging.md).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
prefix=/usr
destdir=
cargo_dir=

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)
            prefix="$2"
            shift 2
            ;;
        --destdir)
            destdir="$2"
            shift 2
            ;;
        --cargo-dir)
            cargo_dir="$2"
            shift 2
            ;;
        *)
            echo "usage: scripts/install-bsm.sh [--prefix /usr] [--destdir DIR] --cargo-dir TARGET" >&2
            exit 2
            ;;
    esac
done
if [ "$prefix" != /usr ]; then
    echo "install-bsm.sh: --prefix must be /usr (the units and policy files name /usr paths), not $prefix" >&2
    exit 2
fi
if [ -z "$cargo_dir" ]; then
    echo "install-bsm.sh: --cargo-dir is required (the Cargo target directory that holds release/)" >&2
    exit 2
fi

bin="$cargo_dir/release"
for program in bsm bsmctl bsm-helper bsm-host bsm-imgconv; do
    if [ ! -f "$bin/$program" ]; then
        echo "install-bsm.sh: $bin/$program is missing; run make bsm first" >&2
        exit 1
    fi
done

dest="${destdir%/}"
data="$root/data/bsm"

install -Dm755 "$bin/bsm" "$dest/usr/bin/bsm"
install -Dm755 "$bin/bsmctl" "$dest/usr/bin/bsmctl"
install -Dm755 "$data/bin/bsm-set-display-manager" "$dest/usr/bin/bsm-set-display-manager"
# Not user commands: bsm and the helper start these by path.
install -Dm755 "$bin/bsm-helper" "$dest/usr/libexec/bsm/bsm-helper"
install -Dm755 "$bin/bsm-host" "$dest/usr/libexec/bsm/bsm-host"
install -Dm755 "$bin/bsm-imgconv" "$dest/usr/libexec/bsm/bsm-imgconv"
install -Dm755 "$data/bin/bsm-fallback" "$dest/usr/libexec/bsm/fallback"
# bsm.service's hooks around the boot splash (docs/bsm/boot.md).
install -Dm755 "$data/bin/bsm-plymouth-handover" "$dest/usr/libexec/bsm/bsm-plymouth-handover"

# Units. None is enabled by the package.
for unit in "$data"/systemd/*; do
    install -Dm644 "$unit" "$dest/usr/lib/systemd/system/$(basename "$unit")"
done
# The stock plymouth-quit.service runs after bsm, which withdraws the splash itself.
install -Dm644 "$data/plymouth/10-bsm-order.conf" "$dest/usr/lib/systemd/system/plymouth-quit.service.d/10-bsm.conf"
install -Dm644 "$data/sysusers/bsm.conf" "$dest/usr/lib/sysusers.d/bsm.conf"
install -Dm644 "$data/tmpfiles/bsm.conf" "$dest/usr/lib/tmpfiles.d/bsm.conf"
# Applied by bsm.service itself, not at boot (see the file).
install -Dm644 "$data/share/tmpfiles-active.conf" "$dest/usr/share/bsm/tmpfiles-active.conf"
install -Dm644 "$root/bsm/core/protocol/bsm-session-host-v1.xml" "$dest/usr/share/bsm/protocol/bsm-session-host-v1.xml"

install -Dm644 "$data/dbus/org.bingux.Session1.conf" "$dest/usr/share/dbus-1/system.d/org.bingux.Session1.conf"
install -Dm644 "$data/polkit/50-bsm.rules" "$dest/usr/share/polkit-1/rules.d/50-bsm.rules"
# PAM services of the helper (bsm-login, bsm-unlock).
for pam in "$data"/pam/*; do
    install -Dm644 "$pam" "$dest/etc/pam.d/$(basename "$pam")"
done
install -Dm644 "$data/config/bsm.toml" "$dest/etc/bsm/bsm.toml"

for doc in packaging boot manager architecture; do
    install -Dm644 "$root/docs/bsm/$doc.md" "$dest/usr/share/doc/bsm/$doc.md"
done
echo "Installed bsm under ${dest:-/}/usr"
