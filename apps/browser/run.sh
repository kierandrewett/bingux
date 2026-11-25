#!/usr/bin/env bash
# Build (if needed) and start the Bingux browser against the Vivarium engine.
#
#   VIVARIUM_SDK_DIR=/path/to/sdk apps/browser/run.sh [--private] [ADDRESS]...
#
# VIVARIUM_SDK_DIR is a Vivarium SDK folder: vivarium-engine, libvivarium.so (and
# libvivarium.so.0 beside it), libxul.so and include/vivarium.h. Everything else
# has a default:
#
#   BINGUX_BROWSER_PROFILE   the profile, default $XDG_DATA_HOME/bingux-browser/profile
#   BINGUX_BROWSER_REMOTE_SETTINGS (off | security | mozilla) and
#   BINGUX_BROWSER_REMOTE_SETTINGS_SERVER   where revocation data comes from
#   BINGUX_BROWSER_FILE_DIRS extra folders file:// pages may read (path list)
#   BINGUX_BROWSER_BUILD     debug (default; dependencies are optimised) or release
#   CARGO_TARGET_DIR         where Cargo builds, default build/cargo under the repository
#   --no-build               first argument: start what is already built
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)

build=true
if [[ ${1:-} == --no-build ]]; then
    build=false
    shift
fi

if [[ -z ${VIVARIUM_SDK_DIR:-} ]]; then
    echo "run.sh: set VIVARIUM_SDK_DIR to a Vivarium SDK folder (just sdk DEST, in the Vivarium repository)." >&2
    exit 2
fi
if [[ ! -e $VIVARIUM_SDK_DIR/libvivarium.so || ! -e $VIVARIUM_SDK_DIR/vivarium-engine ]]; then
    echo "run.sh: $VIVARIUM_SDK_DIR has no libvivarium.so and vivarium-engine: it is not an SDK." >&2
    exit 2
fi
if [[ ! -e $VIVARIUM_SDK_DIR/libvivarium.so.0 ]]; then
    echo "run.sh: $VIVARIUM_SDK_DIR/libvivarium.so.0 is missing; the loader needs it (ln -s libvivarium.so libvivarium.so.0 in the SDK)." >&2
    exit 2
fi

# The engine refuses to start from a folder (or file, or ancestor) that group or
# others can write to, or that is not yours or root's: say so before it does.
if [[ -n $(find "$VIVARIUM_SDK_DIR" "$VIVARIUM_SDK_DIR/vivarium-engine" -maxdepth 0 -perm /022 2>/dev/null) ]]; then
    echo "run.sh: $VIVARIUM_SDK_DIR (or its engine) is writable by group or others; the engine will refuse to start from it." >&2
    echo "run.sh: chmod -R go-w '$VIVARIUM_SDK_DIR' (a development tree can set VV_TRUST_ENGINE_PATH=1 instead)." >&2
fi

export VIVARIUM_SDK_DIR
export LD_LIBRARY_PATH="$VIVARIUM_SDK_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
data=${XDG_DATA_HOME:-$HOME/.local/share}
export BINGUX_BROWSER_PROFILE=${BINGUX_BROWSER_PROFILE:-$data/bingux-browser/profile}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$root/build/cargo}
mkdir -p "$BINGUX_BROWSER_PROFILE"

profile=${BINGUX_BROWSER_BUILD:-debug}
flags=()
dir=debug
if [[ $profile == release ]]; then
    flags+=(--release)
    dir=release
fi

if $build; then
    cargo build "${flags[@]}" --features vivarium --manifest-path "$here/Cargo.toml"
fi
exec "$CARGO_TARGET_DIR/$dir/bingux-browser" "$@"
