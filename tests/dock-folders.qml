import QtQuick
import Quickshell
import Quickshell.Io

// Dock folders against the production Dock, DockFolderPopup and preferences.
// DOCK_FOLDERS_SCENARIO picks one scenario:
//   model    - the dock's folder operations, the one-app collapse, saving to
//              the configuration file and cleaning up an older file.
//   pointer  - the same through a real pointer and keyboard: the runner
//              (tests/dock-folders.py) performs each DOCK_FOLDERS_STEP action
//              in a private headless Sway session.
// The Dock uses test desktop entries, so the result does not depend on the
// applications installed on the machine.
ShellRoot {
    id: test
    readonly property string scenario: Quickshell.env("DOCK_FOLDERS_SCENARIO") || "model"
    readonly property var appIds: ["fx-a", "fx-b", "fx-c", "fx-d", "fx-e", "fx-f"]
    property var entries: ({})
    property var failures: []
    property var steps: []
    property int stepIndex: -1
    property string lastConfig: ""
    property var before: []
    property var pair: []

    function check(condition, message) {
        if (!condition) {
            failures.push(message);
            console.error("FAIL: " + message);
        }
        return condition;
    }
    function same(actual, expected, message) {
        return check(JSON.stringify(actual) === JSON.stringify(expected), message + ": expected " + JSON.stringify(expected) + ", got " + JSON.stringify(actual));
    }
    // An action for the runner: pointer, press, release, keys, type or shot.
    function act(action) {
        console.info("DOCK_FOLDERS_STEP " + JSON.stringify(action));
    }

    function makeEntry(id, name) {
        return {
            id: id,
            startupClass: id,
            name: name,
            icon: "applications-other",
            actions: [],
            noDisplay: false,
            command: [],
            execString: "",
            workingDirectory: "",
            execute: () => {}
        };
    }
    Component.onCompleted: {
        const made = {};
        appIds.forEach((id, index) => made[id] = makeEntry(id, "Test app " + "ABCDEF"[index]));
        entries = made;
        build();
        stepIndex = 0;
    }

    QtObject {
        id: manager
        property var activeToplevel: null
        property QtObject toplevels: QtObject {
            property var values: []
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    Dock {
        id: dock
        testManager: manager
        // Without a saved dock_apps layout the pins come from here and the
        // dock's own state file: the path of a desktop that never saved one.
        settings: ({
                pinnedApps: test.scenario === "legacy" ? test.appIds : []
            })
        function desktopEntryFor(id) {
            return test.entries[id] || null;
        }
        function desktopEntryForWindow(appId, title) {
            return test.entries[normaliseAppId(appId || "")] || null;
        }
    }

    // The saved file, read back through the same program the shell saves with.
    Process {
        id: configReader
        property string key: "desktop.dock_folders"
        command: [Python.config, "get", key]
        stdout: StdioCollector {
            onStreamFinished: test.lastConfig = text
        }
    }
    // Reads a key of the saved file and keeps reading until the step moves on:
    // the shell writes the file in the background, so one read can run first.
    property bool polling: false
    function readConfig(key) {
        lastConfig = "";
        configReader.key = key;
        polling = true;
        poll.restart();
        configReader.running = false;
        configReader.running = true;
    }
    Timer {
        id: poll
        interval: 300
        repeat: true
        running: test.polling
        onTriggered: if (!configReader.running)
            configReader.running = true
    }
    function savedFolders() {
        try {
            return JSON.parse(lastConfig);
        } catch (error) {
            return null;
        }
    }

    function folder() {
        return dock.folders[0] || null;
    }
    function tileIds() {
        return dock.appGroups.map(group => group.id);
    }
    function tile(id) {
        for (let index = 0; index < dock.testItems.count; index++)
            if (dock.testItems.itemAt(index).currentGroup.id === id)
                return dock.testItems.itemAt(index);
        return null;
    }
    function centre(id) {
        const item = tile(id);
        return dock.screenPoint(item, item.width / 2, item.height / 2);
    }
    function popup() {
        return dock.folderPopup;
    }
    function popupOpen() {
        return !!popup() && popup().visible && popup().flightProgress >= 1 && !popup().flightRunning;
    }
    function cellCentre(index) {
        const p = popup();
        return Qt.point(p.panelX + p.cellX(index % p.columns) + p.cellWidth / 2, p.panelY + p.cellY(Math.floor(index / p.columns) % 3) + p.expandedIconSize / 2 + 6);
    }
    function move(point) {
        act({
            "do": "pointer",
            "x": Math.round(point.x),
            "y": Math.round(point.y)
        });
    }
    function drag(from, to, hold) {
        // Move in steps, so Qt sees a drag, and hold at the end for the dwell.
        act({
            "do": "drag",
            "from": [Math.round(from.x), Math.round(from.y)],
            "to": [Math.round(to.x), Math.round(to.y)],
            "hold": hold || 0
        });
    }

    function add(name, run, until, verify) {
        steps.push({
            name,
            run,
            until,
            verify
        });
    }
    function seed(folders, order) {
        BinguxPreferences.saveDesktop({
            dockApps: {
                pinnedApps: appIds.slice(),
                order: order || appIds.slice()
            },
            dockFolders: folders || []
        });
    }

    function build() {
        add("preferences load", null, () => BinguxPreferences.loaded && dock.startupReady);
        if (scenario === "legacy") {
            add("six pinned apps from the settings", null, () => dock.testItems.count === 6);
            buildLegacy();
            return;
        }
        add("seed six apps", () => seed(), () => dock.testItems.count === 6 && tileIds().join() === appIds.join());
        if (scenario === "model")
            buildModel();
        else
            buildPointer();
    }

    function buildModel() {
        add("create a folder from two apps", () => check(dock.createFolder("fx-b", "fx-a"), "createFolder accepted"), () => dock.folders.length === 1 && dock.testItems.count === 5, () => {
            const created = folder();
            same(created.apps, ["fx-a", "fx-b"], "target first, then the dropped app");
            same([created.name, created.color.length > 0], ["Folder", true], "name and colour set");
            same(tileIds(), [created.id, "fx-c", "fx-d", "fx-e", "fx-f"], "the folder takes the target's slot");
            check(/^folder:[a-z0-9]+$/.test(created.id), "the id has the shape the schema allows: " + created.id);
            check(dock.isPinned(dock.appGroups[0]), "a folder sits in the pinned section");
        });
        add("refuse an app on itself and a folder as an app", () => {
            check(!dock.createFolder("fx-c", "fx-c"), "an app cannot make a folder with itself");
            check(!dock.addToFolder(folder().id, folder().id, 0), "a folder cannot be added to a folder");
            check(!dock.addToFolder("fx-a", folder().id, 0), "an app already in the folder is refused");
        }, () => true, () => same(dock.folders.length, 1, "still one folder"));
        add("add an app at an index", () => check(dock.addToFolder("fx-c", folder().id, 1), "addToFolder accepted"), () => folder().apps.length === 3, () => {
            same(folder().apps, ["fx-a", "fx-c", "fx-b"], "inserted at index 1");
            same(tileIds(), [folder().id, "fx-d", "fx-e", "fx-f"], "the member leaves the dock row");
        });
        add("reorder inside the folder", () => check(dock.moveInFolder(folder().id, "fx-c", 2), "moveInFolder accepted"), () => folder().apps[2] === "fx-c", () => same(folder().apps, ["fx-a", "fx-b", "fx-c"], "moved to the end"));
        add("rename trims, refuses empty and long names", () => {
            check(dock.renameFolder(folder().id, "  Work  "), "rename accepted");
            check(!dock.renameFolder(folder().id, "   "), "empty name refused");
            check(!dock.renameFolder(folder().id, "x".repeat(65)), "65 characters refused");
        }, () => folder().name === "Work", () => same(folder().name, "Work", "name trimmed and the refused ones ignored"));
        add("recolour accepts a listed colour only", () => {
            check(dock.recolourFolder(folder().id, "green"), "recolour accepted");
            check(!dock.recolourFolder(folder().id, "magenta"), "unknown colour refused");
        }, () => folder().color === "green", () => same(folder().color, "green", "colour"));
        add("the file holds the folder", () => readConfig("desktop.dock_folders"), () => savedFolders() && savedFolders().length === 1 && savedFolders()[0].name === "Work", () => {
            same(savedFolders()[0], folder(), "saved folder equals the dock's folder");
        });
        add("take an app out", () => check(dock.removeFromFolder(folder().id, "fx-b"), "removeFromFolder accepted"), () => folder() && folder().apps.length === 2, () => {
            same(folder().apps, ["fx-a", "fx-c"], "members");
            same(tileIds(), [folder().id, "fx-b", "fx-d", "fx-e", "fx-f"], "the app lands just after its folder");
        });
        add("taking out the second to last app dissolves the folder", () => check(dock.removeFromFolder(folder().id, "fx-a"), "removeFromFolder accepted"), () => dock.folders.length === 0, () => {
            same(tileIds(), ["fx-c", "fx-a", "fx-b", "fx-d", "fx-e", "fx-f"], "the remaining app takes the folder's slot, then the one taken out");
            same(dock.pinnedApps.slice().sort(), test.appIds.slice(), "every app is still pinned");
        });
        add("the file holds no folder", () => readConfig("desktop.dock_folders"), () => savedFolders() && savedFolders().length === 0, () => {
            same(savedFolders(), [], "saved folders");
        });
        add("ungroup restores the apps in folder order", () => {
            check(dock.createFolder("fx-f", "fx-e"), "createFolder accepted");
        }, () => dock.folders.length === 1);
        add("ungroup", () => check(dock.ungroupFolder(folder().id), "ungroupFolder accepted"), () => dock.folders.length === 0, () => {
            same(tileIds(), ["fx-c", "fx-a", "fx-b", "fx-d", "fx-e", "fx-f"], "back where they were, in folder order");
        });
        add("an older file with an empty and a one-app folder is cleaned", () => {
            BinguxPreferences.saveDesktop({
                dockApps: {
                    pinnedApps: test.appIds.slice(),
                    order: ["folder:oldone", "folder:oldnone", "fx-a", "fx-b", "fx-c", "fx-e", "fx-f"]
                },
                dockFolders: [
                    {
                        "id": "folder:oldone",
                        "name": "Single",
                        "color": "blue",
                        "apps": ["fx-d"]
                    },
                    {
                        "id": "folder:oldnone",
                        "name": "Empty",
                        "color": "red",
                        "apps": []
                    }
                ]
            });
        }, () => dock.folders.length === 0 && dock.testItems.count === 6, () => {
            same(tileIds(), ["fx-d", "fx-a", "fx-b", "fx-c", "fx-e", "fx-f"], "the single app takes its folder's slot");
        });
        add("the cleaned file is saved", () => readConfig("desktop.dock_folders"), () => savedFolders() && savedFolders().length === 0, () => {
            same(savedFolders(), [], "no folders remain in the file");
        });
        add("up to nine apps show in the preview, more show +N", () => {
            check(dock.createFolder("fx-b", "fx-a"), "folder of two");
        }, () => dock.folders.length === 1, () => {
            for (const id of ["fx-c", "fx-d", "fx-e", "fx-f"])
                check(dock.addToFolder(id, folder().id), "add " + id);
        });
        add("the preview of six apps is 3 by 3", null, () => folder().apps.length === 6 && !!tile(folder().id), () => {
            const icon = tile(folder().id).folderPreview;
            same([icon.grid.columns, icon.grid.shown, icon.grid.more], [3, 6, 0], "six apps in three columns");
        });
    }

    function buildLegacy() {
        add("no dock_apps layout is saved", () => readConfig("desktop.dock_apps"), () => lastConfig.length > 0, () => check(dock.preferences.dockApps === null || dock.preferences.dockApps === undefined, "the layout is not in the configuration"));
        add("a folder works without a saved layout", () => check(dock.createFolder("fx-b", "fx-a"), "createFolder accepted"), () => dock.folders.length === 1, () => {
            same(tileIds(), [folder().id, "fx-c", "fx-d", "fx-e", "fx-f"], "the folder takes the target's slot");
            same(folder().apps, ["fx-a", "fx-b"], "members");
        });
        add("take one out and the folder dissolves", () => check(dock.removeFromFolder(folder().id, "fx-a"), "removeFromFolder accepted"), () => dock.folders.length === 0, () => {
            same(tileIds(), ["fx-b", "fx-a", "fx-c", "fx-d", "fx-e", "fx-f"], "the remaining app takes the folder's slot and the other follows");
        });
        add("the folder is gone from the file", () => readConfig("desktop.dock_folders"), () => savedFolders() && savedFolders().length === 0, () => same(savedFolders(), [], "saved folders"));
    }

    function buildPointer() {
        add("drag one app onto another and hold", () => {
            drag(centre("fx-b"), centre("fx-a"), 700);
        }, () => dock.folders.length === 1 && popupOpen(), () => {
            same(folder().apps, ["fx-a", "fx-b"], "the folder holds both apps");
            check(popup().renaming, "a new folder opens with its name ready to type over");
            same(tileIds(), [folder().id, "fx-c", "fx-d", "fx-e", "fx-f"], "tiles");
        });
        add("type a name", () => {
            act({
                "do": "shot",
                "name": "renaming"
            });
            act({
                "do": "type",
                "text": "Work"
            });
            act({
                "do": "keys",
                "keys": ["Return"]
            });
        }, () => folder().name === "Work" && !popup().renaming, () => check(popup().visible, "the card stays open after naming"));
        add("shot of the open folder", () => act({
                "do": "shot",
                "name": "open"
            }), null);
        add("arrow keys move focus and Ctrl+arrow reorders", () => {
            act({
                "do": "keys",
                "keys": ["Right"]
            });
        }, () => popup().keyboardMode, () => same(popup().focusIndex, 0, "the first arrow only shows focus"));
        add("second arrow moves", () => act({
                "do": "keys",
                "keys": ["Right"]
            }), () => popup().focusIndex === 1);
        add("Ctrl+Left moves the app", () => act({
                "do": "keys",
                "keys": ["ctrl+Left"]
            }), () => folder().apps[0] === "fx-b", () => {
            same(folder().apps, ["fx-b", "fx-a"], "reordered by keyboard");
            same(popup().focusIndex, 0, "focus follows the app");
        });
        add("Escape closes the card", () => act({
                "do": "keys",
                "keys": ["Escape"]
            }), () => !popup().visible);
        add("open from the tile with the keyboard", () => dock.openFolder(folder().id, "keyboard"), () => popupOpen() && popup().keyboardMode);
        add("Enter on the first app closes the card", () => act({
                "do": "keys",
                "keys": ["Return"]
            }), () => !popup().visible);
        add("click the folder tile", () => {
            const point = centre(folder().id);
            act({
                "do": "click",
                "x": Math.round(point.x),
                "y": Math.round(point.y)
            });
        }, () => popupOpen());
        add("drag an app out of the card", () => {
            drag(cellCentre(0), Qt.point(80, 60), 200);
        }, () => dock.folders.length === 0, () => {
            same(tileIds(), ["fx-a", "fx-b", "fx-c", "fx-d", "fx-e", "fx-f"], "a folder left with one app is replaced by it and the app taken out follows");
            check(!popup().visible, "the card closes when the folder is gone");
        });
        add("make a folder from c and d", () => drag(centre("fx-d"), centre("fx-c"), 700), () => dock.folders.length === 1 && popupOpen(), () => same(folder().apps, ["fx-c", "fx-d"], "second folder"));
        add("leave the name", () => act({
                "do": "keys",
                "keys": ["Escape"]
            }), () => !popup().renaming, () => check(popup().visible, "Escape cancels renaming first"));
        add("close the card", () => act({
                "do": "keys",
                "keys": ["Escape"]
            }), () => !popup().visible);
        add("drop an app onto the folder tile", () => drag(centre("fx-e"), centre(folder().id), 0), () => folder() && folder().apps.length === 3, () => same(folder().apps, ["fx-c", "fx-d", "fx-e"], "joined at the end"));
        add("open it and drag the first app to the third place", () => {
            dock.openFolder(folder().id);
        }, () => popupOpen());
        add("drag inside the card", () => drag(cellCentre(0), cellCentre(2), 100), () => folder().apps[2] === "fx-c", () => same(folder().apps, ["fx-d", "fx-e", "fx-c"], "reordered by dragging"));
        add("shot with a reordered folder", () => act({
                "do": "shot",
                "name": "reordered"
            }), null);
        add("drag an app onto the dock row", () => {
            const target = centre("fx-f");
            drag(cellCentre(0), Qt.point(target.x - 4, target.y), 100);
        }, () => folder().apps.length === 2, () => {
            const ids = tileIds();
            check(ids.indexOf("fx-d") >= 0 && ids.indexOf("fx-d") === ids.indexOf("fx-f") - 1, "dropped just before the tile it was released over: " + ids.join());
        });
        add("the file holds the folder", () => readConfig("desktop.dock_folders"), () => savedFolders() && savedFolders().length === 1 && savedFolders()[0].apps.length === 2, () => same(savedFolders()[0], folder(), "saved folder equals the dock's folder"));
        add("Escape closes the card left open", () => act({
                "do": "keys",
                "keys": ["Escape"]
            }), () => !popup().visible);
        add("a quick drag over a tile only reorders", () => {
            test.before = tileIds();
            const apps = test.before.filter(id => !id.startsWith("folder:"));
            test.pair = [apps[apps.length - 1], apps[apps.length - 2]];
            drag(centre(test.pair[0]), centre(test.pair[1]), 0);
        }, () => tileIds().join() !== test.before.join(), () => {
            const expected = test.before.slice();
            const first = expected.indexOf(test.pair[0]), second = expected.indexOf(test.pair[1]);
            expected[first] = test.pair[1];
            expected[second] = test.pair[0];
            same(tileIds(), expected, "the dragged tile took the other's slot");
            same(dock.folders.length, 1, "no folder was made without a pause");
        });
        add("the new order is saved", () => readConfig("desktop.dock_apps"), () => {
            try {
                return JSON.parse(lastConfig).order.slice(0, tileIds().length).join() === tileIds().join();
            } catch (error) {
                return false;
            }
        });
        add("shot of the dock row", () => {
            dock.folderPopup.visible = false;
            act({
                "do": "shot",
                "name": "closed"
            });
        }, null);
    }

    Timer {
        id: driver
        interval: 40
        repeat: true
        running: test.stepIndex >= 0
        property var current: null
        property double started: 0
        property double settled: 0
        onTriggered: {
            if (test.stepIndex >= test.steps.length) {
                test.finish();
                return;
            }
            const step = test.steps[test.stepIndex];
            if (current !== step) {
                current = step;
                started = Date.now();
                settled = 0;
                console.info("DOCK_FOLDERS_STEP_NAME " + step.name);
                try {
                    if (step.run)
                        step.run();
                } catch (error) {
                    test.check(false, step.name + ": " + error);
                }
            }
            let ready = false;
            try {
                ready = step.until ? !!step.until() : true;
            } catch (error) {
                ready = false;
            }
            if (!ready) {
                if (Date.now() - started > 8000) {
                    test.check(false, "timed out: " + step.name + " (dragged=" + dock.draggedId + " hover=" + dock.folderHoverId + " ready=" + dock.folderHoverReady + " folders=" + dock.folders.length + " popup=" + (!!test.popup() && test.popup().visible) + " tiles=" + tileIds().join() + " drop=" + dock.dropIndex + " settling=" + dock.settlingDrag + " pointer=" + Math.round(dock.dragPointerX) + "," + Math.round(dock.dragPointerY) + ")");
                    test.finish();
                }
                return;
            }
            // Let animations and the model settle before the next step.
            if (settled === 0) {
                settled = Date.now();
                return;
            }
            if (Date.now() - settled < 300)
                return;
            try {
                if (step.verify)
                    step.verify();
            } catch (error) {
                test.check(false, step.name + ": " + error);
            }
            test.polling = false;
            test.stepIndex++;
        }
    }
    function finish() {
        stepIndex = -1;
        if (failures.length === 0)
            console.info("DOCK_FOLDERS_PASSED " + scenario);
        else
            console.error("DOCK_FOLDERS_FAILED " + failures.length + " failures");
        Qt.quit();
    }
}
