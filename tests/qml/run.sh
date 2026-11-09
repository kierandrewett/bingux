#!/usr/bin/env bash
# Run one QML test fixture against a copy of the shell components.
# Usage: tests/qml/run.sh TestName.qml [qs options...]
#
# Fixture kinds:
# - assertion tests call Qt.quit() themselves; exit status decides pass/fail.
# - scene fixtures draw a state for a screenshot harness and print a *_DONE marker
#   instead of quitting. They pass when the marker appears and no "FAIL:" line is logged.
# - harness-driven fixtures carry a "// harness: <path>" line naming the script that
#   drives them. run.sh prints a SKIP note and exits 0 for those; run the named harness.
#
# Environment: QS_TEST_BIN (quickshell binary), QS_SCENE_SECONDS (scene cap, default 30),
# BUILD_DIR (default build; QML_IMPORT_PATH is set to $BUILD_DIR/qml like the Makefile).
#
# Box requirements for the Search fixtures (SearchIconTest, SearchNavigationTest,
# SearchWebOpenTest): an installed icon theme providing the applications' icons
# (e.g. org.gnome.Epiphany, system-file-manager) and a web browser on PATH
# (e.g. epiphany). Without them they fail with "native icon capture failed",
# "keyboard scroll starts without jumping" and "web search did not foreground its browser".
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/../.." && pwd)"
name="${1:?usage: tests/qml/run.sh TestName.qml [qs options...]}"
shift
harness="$(grep -o '^// harness: .*' "$repo_dir/tests/qml/$name" | head -1 | sed 's#^// harness: ##' || true)"
if [[ -n "$harness" ]]; then
    echo "SKIP $name: driven by $harness"
    exit 0
fi
export QML_IMPORT_PATH="${QML_IMPORT_PATH:-$repo_dir/${BUILD_DIR:-build}/qml}"
test_dir="$(mktemp -d)"
log="$test_dir/qs.log"
trap 'rm -rf "$test_dir"' EXIT
cp -R "$repo_dir"/shell/bingux/. "$test_dir/"
cp "$repo_dir/tests/qml/$name" "$test_dir/$name"
marker="$(grep -o '[A-Z_]*_DONE' "$repo_dir/tests/qml/$name" | head -1 || true)"
status=0
if [[ -n "$marker" ]]; then
    timeout "${QS_SCENE_SECONDS:-30}" "${QS_TEST_BIN:-qs}" -p "$test_dir/$name" "$@" >"$log" 2>&1 || true
    cat "$log"
    if grep -q "FAIL:" "$log" || ! grep -q "$marker" "$log"; then
        status=1
    fi
else
    "${QS_TEST_BIN:-qs}" -p "$test_dir/$name" "$@" || status=$?
fi
exit "$status"
