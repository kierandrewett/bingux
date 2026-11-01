#!/usr/bin/env bash
# Build pkgsmith at the commit named in .github/pkgsmith.ref, with the plugins Bingux uses, into DIR.
#
#   scripts/install-pkgsmith.sh [DIR]        default: ./pkgsmith-bin
#
# pkgsmith has no published release yet, so it is built from source: a Rust toolchain and git are the
# only requirements. DIR then holds `pkgsmith` and the plugin libraries beside it, which is a folder
# pkgsmith searches for plugins. Run DIR/pkgsmith, or set PKGSMITH=DIR/pkgsmith for
# scripts/build-packages.sh.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="$(realpath -m "${1:-$root/pkgsmith-bin}")"
read -r repository commit < <(grep -v '^#' "$root/.github/pkgsmith.ref")

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
git -C "$work" init -q src
git -C "$work/src" fetch -q --depth 1 "https://github.com/$repository.git" "$commit"
git -C "$work/src" checkout -q FETCH_HEAD

# -crt-static makes a build on musl dynamic, which it must be to load plugins; glibc builds are dynamic anyway.
(cd "$work/src" && RUSTFLAGS="-C target-feature=-crt-static" cargo build --release --locked \
    -p pkgsmith -p pkgsmith-plugin-rpm -p pkgsmith-plugin-deb -p pkgsmith-plugin-arch -p pkgsmith-plugin-tar)

mkdir -p "$dest"
install -m755 "$work/src/target/release/pkgsmith" "$dest/pkgsmith"
for plugin in rpm deb arch tar; do
    install -m755 "$work/src/target/release/libpkgsmith_plugin_$plugin.so" "$dest/"
done
"$dest/pkgsmith" plugins | sed -n '/^Loaded:/,$p'
