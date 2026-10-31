#!/usr/bin/env bash
# Install the build dependencies of packaging/debian on Debian 13 (trixie). Run it in the source root with debian/ in place.
#
# The programs use `if let` chains, which need Rust 1.88, and trixie ships 1.85. trixie-backports has a newer Rust
# (1.96 when this was written), so Rust comes from there and everything else from trixie. Cargo.toml says
# rust-version 1.85, which the code does not meet.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

echo "deb http://deb.debian.org/debian trixie-backports main" >/etc/apt/sources.list.d/trixie-backports.list
apt-get update
apt-get install -y -t trixie-backports rustc cargo
apt-get build-dep -y ./
dpkg-checkbuilddeps
