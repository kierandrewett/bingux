# bingux-shell

Quickshell 0.2.1, with the Bingux runtime patches applied, built into one
binary whose entry point knows the Bingux shells.

```text
quickshell/             pinned Quickshell submodule (left pristine)
prepare-quickshell.py   copies the submodule into the build tree and applies
                        ../quickshell-patches/*.patch to the copy
CMakeLists.txt          builds the patched copy, swaps in src/main.cpp and
                        names the executable bingux-shell
src/main.cpp            mode dispatch, archive lookup, environment defaults
src/archive.cpp         unpacks the shell archive into the user's cache
```

`make shell` runs `scripts/pack-shell.py` (the shell archive) and this CMake
project with the Ninja generator (Quickshell's generated D-Bus sources are
not ordered correctly by the Makefile generator). The Makefile sets
`BINGUX_LIBDIR` (`lib` or `lib64`) and the version.

## Behaviour

- `bingux-shell MODE [--source DIR | --archive FILE] [quickshell options]`
  starts one shell: `desktop`, `lock`, `greeter`, `popouts`, `wallpaper`,
  `search`, `switcher`, `emoji` or `capture`.
- Invoked as `bingux`, `bingux-lock`, `bingux-greeter`, ... (the installed
  symlinks) the binary reads argv[0] and picks the matching mode.
- `bingux-shell -p FILE.qml ...` is plain Quickshell with the Bingux
  environment applied; tests and `binguxctl ... ipc` use it.
- `bingux-shell --shell-dir` prints the directory the installed archive was
  unpacked into. `binguxctl` uses it as `BINGUX_CONFIG_PATH`.
- The archive (`<libdir>/bingux/bingux-shell.sxa`) is a tar.xz with
  `manifest.json` and the `shell/` tree. It is unpacked once per content
  hash into `$XDG_CACHE_HOME/bingux/shell/<id>/`, or `/tmp/bingux-shell-<uid>`
  when the cache is not writable. Member paths are checked before extraction.
- Environment defaults (set only when unset and when the path exists):
  `QML_IMPORT_PATH`, `PATH`, `XDG_DATA_DIRS`, `BINGUX_HELPER_DIR`,
  `BINGUX_CONFIG_BIN`, `BINGUX_SETTINGS_BIN`, `BINGUX_AUDIO_METER`,
  `BINGUX_PYTHON=python3`, `BINGUX_QUICKSHELL`
  (the binary itself) and `BINGUX_CONFIG_PATH` (the shell directory).
- The core-dump limit is set to zero before Quickshell starts.

## Updating Quickshell

1. `git -C shell/runtime/quickshell checkout <tag>` and commit the
   new pin.
2. Rebase `shell/quickshell-patches/*.patch`; `prepare-quickshell.py` stops
   on the first patch that no longer applies.
3. `make shell check-session`.
