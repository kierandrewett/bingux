#!/usr/bin/env bash
# Publish the pacman repository that distro/build.sh built to a GitHub release,
# which is where /etc/pacman.d/bingux-os-mirrorlist on an installed system
# looks for it:
#   https://github.com/kierandrewett/bingux/releases/download/pacman-x86_64
#
# usage: distro/publish-repo.sh [--dry-run]
#
# Needs `gh` signed in with write access to the repository. Each run replaces
# the release assets, so the database always matches the packages next to it.
# The packages are unsigned (SigLevel = Optional TrustAll); see distro/README.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_DIR="$ROOT/distro/out/repo"
REPOSITORY="${BINGUX_REPO_SLUG:-kierandrewett/bingux}"
TAG="pacman-x86_64"
DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

ls "$REPO_DIR"/*.pkg.tar.zst >/dev/null 2>&1 || {
    echo "publish-repo: no packages in $REPO_DIR; run distro/build.sh first" >&2
    exit 1
}
for database in bingux-os.db bingux-os.files; do
    [ -e "$REPO_DIR/$database" ] || {
        echo "publish-repo: $REPO_DIR/$database is missing; run distro/build.sh --step packages" >&2
        exit 1
    }
done

# Release assets are plain files; the database names in the repository are symlinks.
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
cp -L "$REPO_DIR"/*.pkg.tar.zst "$REPO_DIR"/bingux-os.db "$REPO_DIR"/bingux-os.db.tar.gz \
    "$REPO_DIR"/bingux-os.files "$REPO_DIR"/bingux-os.files.tar.gz "$stage"/

echo "[publish-repo] $REPOSITORY release $TAG:"
ls -l "$stage"
if [ "$DRY_RUN" = 1 ]; then
    echo "[publish-repo] dry run; nothing uploaded"
    exit 0
fi

gh release view "$TAG" --repo "$REPOSITORY" >/dev/null 2>&1 ||
    gh release create "$TAG" --repo "$REPOSITORY" --title "Bingux OS pacman repository (x86_64)" \
        --notes "Unsigned pacman repository for Bingux OS. Built by distro/build.sh; see distro/README.md."
gh release upload "$TAG" --repo "$REPOSITORY" --clobber "$stage"/*
echo "[publish-repo] done: https://github.com/$REPOSITORY/releases/tag/$TAG"
