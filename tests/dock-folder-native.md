# Check dock folder loading

Run this from the Bingux checkout against a running Gnoblin QEMU guest:

```sh
python3 tests/dock-folder-native.py --run RUN_DIRECTORY --output OUTPUT_DIRECTORY
```

The guest needs one normal output at scale 1, the installed source prefixes
used by Gnoblin's QEMU guide, and at least 12 visible desktop applications.
The output directory must be new. Use `--gnoblin-repo` for a separate Gnoblin
checkout, or `--source` to compare another Dock.qml. `--allow-eager` allows the
original popup state during a baseline comparison; it disables the cold-absence
assertions.

The runner copies production components into a separate native fixture. It
injects a fake toplevel manager and in-memory preferences, without changing
the installed main shell or saved desktop settings. It checks cold empty and
saved folders, invalid opening, dropping before the hover timer, pointer first
opening, outside mousedown, Escape, retained reopening, hover opening, folder
creation, deletion and three engine reload states. The original build fails
the cold-absence assertion. Captures show the first opening and created folder.

Native pointer motion outside the dock does not reach this fixture on the
original build. The paging and drop-coordinate checks call the unchanged
production handler through `dragPoint`; they do not prove the complete native
drag-to-folder path. No multi-monitor, high-DPI, installed-main editor or
hardware frame-pacing claim follows from this fixture.

For source compilation evidence, use a complete cold trace:

```sh
python3 tests/qml-cold-trace.py TRACE.qmltrace --forbid DockFolderPopup.qml
```

The test releases its pointer button and stops its own Quickshell process on
exit. It never restarts host GDM.
