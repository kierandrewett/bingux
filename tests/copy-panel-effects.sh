#!/usr/bin/env bash
# Keep isolated QML fixtures on the production fade helper, with compositor
# policy disabled unless the test supplied its own policy implementation.
set -euo pipefail
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
destination="$1"
cp "$repo_dir"/shell/bingux/{SurfaceFade,SurfaceFades,SurfaceFadeNative,PopupShadow,BackgroundEffect,BackgroundEffects,BackgroundEffectNative}.qml "$destination/"
printf '%s\n' 'singleton SurfaceFades 1.0 SurfaceFades.qml' >>"$destination/qmldir"
printf '%s\n' 'SurfaceFade 1.0 SurfaceFade.qml' 'PopupShadow 1.0 PopupShadow.qml' >>"$destination/qmldir"
printf '%s\n' 'singleton BackgroundEffects 1.0 BackgroundEffects.qml' 'BackgroundEffect 1.0 BackgroundEffect.qml' >>"$destination/qmldir"
if [[ ! -f "$destination/PopupTransitions.qml" ]]; then
    cp "$repo_dir/tests/fixtures/PopupTransitions.qml" "$destination/"
    printf '%s\n' 'singleton PopupTransitions 1.0 PopupTransitions.qml' >>"$destination/qmldir"
fi
if [[ ! -f "$destination/DesktopEditing.qml" ]]; then
    cp "$repo_dir/tests/fixtures/DesktopEditing.qml" "$destination/"
    printf '%s\n' 'singleton DesktopEditing 1.0 DesktopEditing.qml' >>"$destination/qmldir"
fi
