#!/usr/bin/env bash
# Run in the ordinary desktop session, not inside the hardened search daemon.
# Copies only user-visible home paths from the OS index; never crawls the disk.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/bingux"
locate_args=(-N)
if [[ "${1:-}" == "--filter-current" ]]; then
    locate_args+=(-d "$cache_dir/locate.db")
elif [[ $# != 0 ]]; then
    printf 'Usage: %s [--filter-current]\n' "$0" >&2
    exit 2
fi
mkdir -p "$cache_dir"
task_snapshot_dir=$(mktemp -d "$cache_dir/.locate.XXXXXX")
trap 'rm -f -- "$task_snapshot_dir/paths.txt" "$task_snapshot_dir/locate.db"; rmdir -- "$task_snapshot_dir"' EXIT
plocate "${locate_args[@]}" -- "$HOME/" |
    awk -v prefix="$HOME/" '
        NR == FNR { excluded[tolower($0)] = 1; next }
        index($0, prefix) == 1 {
            count = split($0, parts, "/"); skip = 0;
            for (i = 1; i <= count; i++)
                if (parts[i] ~ /^\./ || tolower(parts[i]) in excluded) skip = 1;
            if (tolower($0) ~ /\.(o|obj|pyc|pyo|class|tsbuildinfo)$/) skip = 1;
            if (!skip) print;
        }
    ' "$script_dir/search-excluded-directories.txt" - > "$task_snapshot_dir/paths.txt"
plocate-build -p "$task_snapshot_dir/paths.txt" "$task_snapshot_dir/locate.db"
chmod 600 "$task_snapshot_dir/locate.db"
mv -- "$task_snapshot_dir/locate.db" "$cache_dir/locate.db"
printf 'Updated Bingux OS-index snapshot: %s\n' "$cache_dir/locate.db"
