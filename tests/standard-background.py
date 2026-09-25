#!/usr/bin/env python3
"""Real dock and context-menu blur through ext-background-effect-v1."""

import json
import os
from pathlib import Path
import subprocess
import time
from PIL import Image, ImageChops, ImageStat

assert os.environ.get("WAYLAND_DISPLAY", "").startswith("gnoblin-gs-")
repo = Path(__file__).resolve().parents[1]
gnoblin = repo.parent / "gnoblin"
expected_standard = os.environ.get("EXPECT_STANDARD", "1") == "1"
config = Path(os.environ["XDG_CONFIG_HOME"])
root = config / "gnoblin"
scripts = root / "scripts"
scripts.mkdir(parents=True, exist_ok=True)
(root / "init.lua").write_text("""return {
 shell = {['layer-animation'] = 'none'},
 ['window-rules'] = {{match = {layer = '^bingux-dock$'}, blur = 48},
                     {match = {layer = '^gnoblin-shell-popup$'}, blur = 24}},
}""")
(scripts / "standard-observer.js").write_text("""import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
export default function(api) {
 const timer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
  const result = [];
  for (const actor of global.get_window_actors()) {
   const namespace = Meta.gnoblin_layer_namespace(actor.meta_window);
   if (!['bingux-dock', 'gnoblin-shell-popup'].includes(namespace)) continue;
   const effects = [];
   function walk(item) {
    const effect = item.get_effect('gnoblin-standard-background-blur');
    if (effect) effects.push({radius:effect.radius});
    for (const child of item.get_children()) walk(child);
   }
   walk(actor);
   result.push({namespace, effects, legacy:!!actor.get_effect('gnoblin-window-blur'),
     legacyRadius:actor.get_effect('gnoblin-window-blur')?.radius});
  }
  GLib.file_set_contents(GLib.build_filenamev([GLib.get_user_config_dir(),'standard-state.json']), JSON.stringify(result));
  return GLib.SOURCE_CONTINUE;
 });
 api.addCleanup(() => GLib.source_remove(timer));
}
""")
subprocess.run([str(gnoblin / "src/tools/gnoblinctl"), "reload"], check=True)
# Inspection aliases leave the production dock behaviour and input path intact.
components = config / "components"
components.mkdir()
for component in (repo / "shell/bingux").iterdir():
    if component.name != "Dock.qml":
        (components / component.name).symlink_to(component)
dock_source = (repo / "shell/bingux/Dock.qml").read_text()
dock_source = dock_source.replace(
    "required property var settings", "required property var settings\n    property alias testItems: dockItems", 1
)
dock_source = dock_source.replace(
    "id: dockButton", "id: dockButton\n                    property alias testMenu: appMenu", 1
)
(components / "Dock.qml").write_text(dock_source)
fixture = config / "standard-background.qml"
fixture.write_text(
    """import QtQuick
import QtTest
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "BINGUX"
ShellRoot {
 id: root
 property bool flat: false
 Component.onCompleted: {
  BinguxPreferences.data = Object.assign({}, BinguxPreferences.data, {desktop:
   Object.assign({}, BinguxPreferences.data.desktop, {dockApps:{pinnedApps:["steam", "org.localsend.localsend_app"], order:[]}})});
  dock.refreshAppGroups();
 }
 IpcHandler { target: "standard"
  function reference(value: bool): void { root.flat = value; }
  function menu(): void { input.mouseClick(input.findChild(dock.contentItem, "dockApplicationIcon"), 20, 20, Qt.RightButton); }
  function count(): int { return dock.appGroups.length; }
  function bounds(): string {
   const menu = dock.testItems.itemAt(0).testMenu;
   return JSON.stringify({
    "bingux-dock": {x:dock.editSurface.x, y:dock.screen.height-dock.height+dock.editSurface.y, width:dock.editSurface.width,height:dock.editSurface.height},
    "gnoblin-shell-popup": {x:menu.panelX, y:menu.panelY, width:menu.popupWidth,height:menu.popupHeight}
   });
  }
  function diagnostic(): string { return JSON.stringify({flat:root.flat,width:dock.width,height:dock.height,visible:dock.visible,screen:!!dock.screen,groups:dock.appGroups.length,geometry:[dock.editSurface.x,dock.editSurface.width,dock.editSurface.height]}); }
 }
 TestCase { id: input; name: "StandardBlurInput"; when: false; function test_manual() {} }
 PanelWindow {
  anchors { top:true; bottom:true; left:true; right:true }
  WlrLayershell.layer: WlrLayer.Background
  color: "#888888"
  Canvas {
   id: pattern
   anchors.fill: parent
   onPaint: {
    const ctx = getContext("2d");
    ctx.fillStyle = "#888888"; ctx.fillRect(0,0,width,height);
    if (!root.flat) for (let y=0;y<height;y+=4) for (let x=0;x<width;x+=4) {
     ctx.fillStyle = (x/4+y/4)%2 ? "#eeeeee" : "#222222"; ctx.fillRect(x,y,4,4);
    }
   }
   Connections { target: root; function onFlatChanged() { pattern.requestPaint(); } }
  }
 }
 Dock { id: dock; screen: Quickshell.screens[0]; settings: ({pinnedApps:["steam", "org.localsend.localsend_app"]}) }
}
""".replace("BINGUX", components.as_uri())
)
qs = os.environ.get("QS_TEST_BIN", "qs")
output = Path("/tmp/bingux-standard-background")
output.mkdir(exist_ok=True)
log = (config / "standard-qml.log").open("w")
process = subprocess.Popen([qs, "-p", str(fixture)], stdout=log, stderr=log)


def ipc(method, *args):
    return subprocess.check_output(
        [qs, "ipc", "-p", str(fixture), "call", "standard", method, *map(str, args)], text=True
    ).strip()


def capture(name):
    path = output / (name + ".png")
    subprocess.run(["grim", str(path)], check=True)
    return Image.open(path).convert("RGB")


def state():
    return json.loads((config / "standard-state.json").read_text())


def wait(predicate):
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        try:
            result = state()
            if predicate(result):
                return result
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
    print("client status", process.poll(), flush=True)
    print(ipc("diagnostic"), flush=True)
    raise AssertionError(state())


try:
    wait(
        lambda items: any(
            item["namespace"] == "bingux-dock" and (bool(item["effects"]) if expected_standard else item["legacy"])
            for item in items
        )
    )
    assert int(ipc("count")) == 2, "Both installed icons must be present"
    time.sleep(1)
    ipc("menu")
    settled = wait(
        lambda items: any(
            item["namespace"] == "gnoblin-shell-popup"
            and (bool(item["effects"]) if expected_standard else item["legacy"])
            for item in items
        )
    )
    time.sleep(1)
    checker = capture("checker")
    ipc("reference", "true")
    deadline = time.monotonic() + 5
    while True:
        time.sleep(0.1)
        reference = capture("reference")
        if max(ImageStat.Stat(ImageChops.difference(checker, reference).crop((0, 200, 100, 300))).mean) > 50:
            break
        assert time.monotonic() < deadline, ("Reference frame did not arrive", ipc("diagnostic"))
    difference = ImageChops.difference(checker, reference)
    bounds = json.loads(ipc("bounds"))
    dock_bounds = bounds["bingux-dock"]
    centre = int(dock_bounds["x"] + dock_bounds["width"] / 2)
    top = int(dock_bounds["y"])
    bottom = int(dock_bounds["y"] + dock_bounds["height"])
    for edge, y in [("top", top - 5), ("bottom", bottom + 2)]:
        strip = difference.crop((centre - 20, y, centre + 20, y + 3))
        contrast = min(ImageStat.Stat(strip).mean)
        assert contrast > 80, (edge, "dock blur extends into transparent padding", contrast)
    for item in settled:
        assert bool(item["effects"]) == expected_standard and item["legacy"] != expected_standard, item
        for effect in item["effects"] if expected_standard else [{"radius": item["legacyRadius"]}]:
            expected = 48 if item["namespace"] == "bingux-dock" else 24
            assert effect["radius"] == expected, item
            r = bounds[item["namespace"]]
            # Keep the exterior rounded edge and shadows out of the interior comparison.
            box = (int(r["x"] + 24), int(r["y"] + 24), int(r["x"] + r["width"] - 24), int(r["y"] + r["height"] - 12))
            assert box[2] > box[0] and box[3] > box[1], box
            stat = ImageStat.Stat(difference.crop(box))
            assert max(high for low, high in stat.extrema) <= 4, (item, stat.extrema, stat.mean)
    print(
        f"PASS: actual dock icons and right-click menu use {'standard regions' if expected_standard else 'legacy fallback'}, preserve radii 48/24, and leak no sharp backdrop",
        flush=True,
    )
    print(settled, flush=True)
finally:
    process.terminate()
    process.wait(timeout=5)
    log.close()
    diagnostic = (config / "standard-qml.log").read_text()
    print(diagnostic[-6000:])
