#!/usr/bin/env python3
"""Fail when Bingux UI code styles a box outside the token system.

Every spacing, radius, size, colour, line, shadow and text style comes from
crates/ui/src/style/ (tokens.rs and recipes.rs) and
crates/ui/src/typography.rs. Boxes are styled with `.tw(&tokens,
RECIPE)` or `.tw(&tokens, &[px(S4), ...])`. Calling Freya's styling methods
directly is how a UI drifts, so this script fails on them.

Rules, for every file except the style system itself:
  styling calls      .padding .spacing .margin .corner_radius .rounded*
                     .background .border .shadow .font_size .font_weight .color
                     .direction .content .cross_align .main_align .center
                     .overflow                                      not allowed
  colour fields      tokens.text, tokens.accent, ...                 use Paint or Ink
  colour values      Color::..., FontWeight::...                     not allowed
  sizes              .width/.height/.min_*/.max_* with a number      use a Dim
                     literal (other than 0) or a token's .px()       utility
  icon sizes         icons::icon(glyph, SIZE, ..) needs IconSize::

A run-time value (a measured width, a drag offset, an animation) may still be
passed to Size::px, offset_x, offset_y, opacity and rotate.

One more rule covers every Rust file under packages/ (the Freya vendor copies
and build output excepted): a window starts only through
`bingux_ui::app::Application::run`, whose private `launch_application` adds the
inspector plugin, so every bingux-ui app has the inspector and its
Ctrl+Alt+Shift+I shortcut by construction. A bare `LaunchConfig`, `freya::launch`
or an import of Freya's `launch` anywhere else fails here.

One more covers the QML shell (shell/bingux): every icon name its QML, JavaScript and Python
asks for, and every one the daemons send it, is a Bingux icon (shell/bingux/icons) or is
mapped to one in BinguxIconAliases.js; none is left to the Adwaita theme. `--self-test` runs
that check on a small tree and fails unless a bogus name is reported.
"""

import re
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from crate_dirs import crate_dir  # noqa: E402

# The rules of design/rules.json each kind of finding holds, and the words that tell the kinds
# apart in this script's output (read by scripts/rules-check.py; a kind with no ID holds no rule yet).
RULE_IDS = {
    "direct-styling": ["DR-03"],
    "raw-colour": ["DR-03"],
    "ink-near-fill": ["CL-04"],
    "fixed-size": ["DR-03"],
    "header-size": ["SZ-02"],
    "icon-size": ["SZ-03"],
    "window-launch": ["IN-01"],
    "blocking-call": ["MO-03", "BH-03"],
    "panic-manifest": ["DG-04"],
    "raw-press": ["PR-01", "FC-04"],
    "app-about": [],
    "shell-icon": [],
    "own-loop": ["MO-04", "MO-10"],
}
RULE_MESSAGES = {
    "direct-styling": r"styles a box directly",
    "raw-colour": r"is a raw colour|Color::TRANSPARENT|is a raw value",
    "shell-icon": r"asks for the icon|BinguxIconAliases\.js|BinguxIconIndex\.js|SHELL_ICONS_DYNAMIC",
    "ink-near-fill": r"sits near bg\(Paint::",
    "fixed-size": r"uses a fixed size",
    "header-size": r"sizes a header element|height must come from",
    "icon-size": r"icon size .* is not an IconSize",
    "window-launch": r"starts a window without the inspector",
    "blocking-call": r" blocks; use the wrappers| blocks the UI thread|in the initial value of a state or memo",
    "panic-manifest": r"panic = \"abort\"|must say panic = \"unwind\"|must patch freya-core",
    "raw-press": r"a press handler on a bare box",
    "app-about": r"starts an Application without",
    "own-loop": r"a looping animation",
}

ROOT = Path(__file__).resolve().parents[1]
SOURCES = [
    ROOT / "crates/ui/src",
    ROOT / "crates/ui/examples",
    ROOT / "apps/settings/src",
    ROOT / "apps/files/src",
    ROOT / "apps/taxonomy/src",
]

# Every directory that holds the repository's own Rust crates. vendor/ (the Freya
# forks) is left out on purpose.
RUST_DOMAINS = ("apps", "crates", "services", "bsm", "tools")


def own_rust_files(pattern: str) -> list[Path]:
    """Rust files of the repository's own crates matching `pattern`, build output excepted."""
    found = []
    for domain in RUST_DOMAINS:
        for path in (ROOT / domain).glob(pattern):
            if "target" not in path.relative_to(ROOT).parts:
                found.append(path)
    return sorted(found)


# The style system defines the values; the palette and icon table hold raw
# colours and SVG; the snapshot runner and the icon contact-sheet example are
# test tooling (they need exact pixel sizes and plain black and white).
SKIP_DIRS = {ROOT / "crates/ui/src/style"}
SKIP_FILES = {
    ROOT / "crates/ui/src/theme.rs",
    ROOT / "crates/ui/src/typography.rs",
    ROOT / "crates/ui/src/icons.rs",
    ROOT / "crates/ui/src/snapshot.rs",
    # The tooltip size tests hang a bubble from boxes of exact, arbitrary
    # pixel widths narrower than its text: that is what they test.
    ROOT / "crates/ui/src/controls/tooltip_layout_tests.rs",
    ROOT / "crates/ui/examples/icon_sheet.rs",
    ROOT / "crates/ui/src/surface.rs",
    # Contrast measurement: colour maths and the matrix that reads raw tokens.
    ROOT / "crates/ui/src/contrast.rs",
    ROOT / "crates/ui/src/contrast_matrix.rs",
    ROOT / "crates/ui/src/charts/palette.rs",
    # The tones of the colour file icons are derived from, and checked against,
    # the raw token colours of every ground a row is drawn on.
    ROOT / "crates/ui/src/files/colour.rs",
    ROOT / "crates/ui/src/files/colour_tests.rs",
    # Test fixtures that measure exact geometry or paint raw probe colours:
    # RTL mirror probes, a bright-red dialog under a menu, a blue test PNG.
    ROOT / "crates/ui/src/controls/hit_area_tests.rs",
    ROOT / "crates/ui/src/files/preview_step_tests.rs",
    ROOT / "crates/ui/src/controls/direction_tests.rs",
    ROOT / "crates/ui/src/shell/menu_over_dialog_tests.rs",
    ROOT / "crates/ui/src/preview/behaviour_tests.rs",
    ROOT / "crates/ui/src/inspector/state_checks/tests.rs",
    ROOT / "crates/ui/src/charts/svg.rs",
}

STYLING = re.compile(
    r"\.(padding|spacing|margin|corner_radius|rounded(?:_[a-z0-9]+)?|background|border|shadow|"
    r"font_size|font_weight|color|direction|content|cross_align|main_align|overflow)\(|(?<![\w\]])\.center\(\)"
)
COLOUR_FIELD = re.compile(
    r"\btokens\.(bg|surface|sidebar|elevated|control|separator|card|hover|pressed|selection|border|"
    r"outline|text|muted|bar_divider|success|warning|danger|on_accent|on_danger|on_success|on_warning|"
    r"on_floating|accent_ink|danger_ink|success_ink|warning_ink|danger_on_floating|focus_ring|error_ring|"
    r"tooltip_surface|tooltip_outline|popover|accent)\b(?!_name)"
)
RAW_VALUE = re.compile(r"\b(Color::[A-Za-z_]+|FontWeight::[A-Z_]+)")
SIZE_CALL = re.compile(r"\.(width|height|min_width|max_width|min_height|max_height)\(")
LITERAL = re.compile(r"(?<![\w.])(\d+\.\d+|\d+)(?![\w.])")
ICON = re.compile(r"icons::icon\(")
# A coloured fill and the ink drawn on it must come from the token pairs the
# contrast matrix measures (OnAccent on Accent, OnDanger on Danger, ...). A
# plain text ink on one of these fills is a pair outside the matrix.
FILLED = re.compile(r"\bbg\(Paint::(Accent|Danger|Success|Warning|Floating)\)")
PLAIN_INK = re.compile(r"\b(?:Ink|Paint)::(Text|Muted)\b")
PAIR_WINDOW = 12  # lines either side of the fill


def args_of(text: str, open_index: int) -> str:
    depth = 0
    for index in range(open_index, len(text)):
        if text[index] == "(":
            depth += 1
        elif text[index] == ")":
            depth -= 1
            if depth == 0:
                return text[open_index + 1 : index]
    return text[open_index + 1 :]


def strip_comments(text: str) -> str:
    # Blank out line comments so doc examples are not checked; keep offsets.
    return re.sub(r"//[^\n]*", lambda m: " " * len(m.group(0)), text)


def check(path: Path) -> list[str]:
    text = strip_comments(path.read_text())
    problems = []
    rel = path.relative_to(ROOT)

    def report(index: int, message: str) -> None:
        problems.append(f"{rel}:{text.count(chr(10), 0, index) + 1}: {message}")

    for match in STYLING.finditer(text):
        # `Paint::X.color(&tokens)` and `Ink::X.color(&tokens)` resolve a
        # token; only Freya's `.color(value)` builder is styling.
        args = args_of(text, match.end() - 1).strip() if match.group(1) else ""
        if match.group(1) == "color" and (args.startswith("&") or args == "tokens"):
            continue
        # Component methods that share a name (a dialog's `.content(..)`).
        if match.group(1) == "content" and not args.startswith("Content::"):
            continue
        report(
            match.start(),
            f"{match.group(0).rstrip('(')}(..) styles a box directly; use .tw(&tokens, ..)",
        )
    for match in COLOUR_FIELD.finditer(text):
        report(
            match.start(), f"{match.group(0)} is a raw colour; use Paint::.. or Ink::.."
        )
    for match in RAW_VALUE.finditer(text):
        if match.group(0) in ("Color::TRANSPARENT",):
            report(match.start(), "Color::TRANSPARENT; use Paint::Transparent")
            continue
        report(match.start(), f"{match.group(0)} is a raw value; use a token")
    lines = text.split("\n")
    for match in FILLED.finditer(text):
        line = text.count("\n", 0, match.start())
        near = "\n".join(lines[max(0, line - PAIR_WINDOW) : line + PAIR_WINDOW + 1])
        wrong = PLAIN_INK.search(near)
        if wrong:
            report(
                match.start(),
                f"{wrong.group(0)} sits near bg(Paint::{match.group(1)}); text on a coloured fill uses "
                f"Paint::On{match.group(1)} / Ink::On{match.group(1)} (the contrast matrix's pair)",
            )
    for match in SIZE_CALL.finditer(text):
        args = args_of(text, match.end() - 1)
        if args.strip().startswith(("Size::percent(", "Size::flex(")):
            continue
        numbers = [
            m.group(1) for m in LITERAL.finditer(args) if m.group(1) not in ("0", "0.0")
        ]
        if numbers or ".px()" in args:
            report(
                match.start(),
                f".{match.group(1)}({args.strip()[:40]}) uses a fixed size; use w(Dim::..)/h(Dim::..)",
            )
    # A header's icon buttons take their size from HEADER_BUTTON (the control
    # height) inside HeaderBar; sizing one in a header drifts the bar.
    for match in re.finditer(r"\.(leading|trailing)\(", text):
        if re.search(r"\.size\(", args_of(text, match.end() - 1)):
            report(
                match.start(),
                f".{match.group(1)}(..) sizes a header element; header buttons use HEADER_BUTTON",
            )
    for match in ICON.finditer(text):
        parts = [part.strip() for part in args_of(text, match.end() - 1).split(",")]
        if len(parts) > 1 and not parts[1].startswith(
            ("IconSize::", "size", "self.size")
        ):
            if LITERAL.search(parts[1]):
                report(match.start(), f"icon size {parts[1]} is not an IconSize")
    # `// ui-lint: allow(reason)` on a line, or the line above it, exempts that
    # line: for a size or inset that is measured at run time, not a design value.
    raw = path.read_text().split("\n")
    exempt = {
        number + offset
        for number, line in enumerate(raw, 1)
        if re.search(r"ui-lint: allow\([^)]+\)", line)
        for offset in (0, 1)
    }
    return [problem for problem in problems if int(problem.split(":")[1]) not in exempt]


# The only files that may touch Freya's launch API: the app loop that builds the
# one launch configuration, and the font helper it calls.
LAUNCH_ALLOWED = {
    ROOT / "crates/ui/src/app.rs",
    ROOT / "crates/ui/src/theme.rs",
}
LAUNCH = re.compile(
    r"\bLaunchConfig\b|\bfreya(?:_winit)?::(?:\w+::)*launch\b|\blaunch_with_params\b|"
    r"\buse\s+freya[\w:]*::\{[^}]*\blaunch\b|\buse\s+freya[\w:]*::launch\b"
)


def check_launch(path: Path) -> list[str]:
    text = strip_comments(path.read_text())
    rel = path.relative_to(ROOT)
    return [
        f"{rel}:{text.count(chr(10), 0, match.start()) + 1}: {match.group(0)} starts a window without the inspector; "
        "use bingux_ui::app::Application::run"
        for match in LAUNCH.finditer(text)
    ]


# --- Nothing blocks the UI thread -------------------------------------------
#
# A frame is 16 ms. A process, a file read, a sleep or a lock held across work
# on the UI thread is a dropped frame, or a slide that glitches. Blocking work
# runs on the worker pool (bingux_ui::task, bingux_ui::runtime) and comes back
# through State. Two rules, for apps/settings/src and
# crates/ui/src (not the inspector, which is developer tooling):
#
#   1. The raw primitives below may appear only in the files of BLOCKING_ALLOWED,
#      each with the reason it is safe. Everything else goes through the
#      guarded wrappers in bingux_ui::runtime (run, read_to_string, read_file,
#      read_dir, metadata), which report a call made on the UI thread when it
#      happens (`[ui-blocking]`, see scripts/frame-time.py).
#   2. No `render` body may call a primitive or a wrapper at all, in any file.
#
# Keep the allowlist short. A file that is not on it and needs a primitive
# should call the wrapper instead.
BLOCKING = re.compile(
    r"\bCommand::new\(|\b(?:std::)?fs::(?:read|read_to_string|read_dir|metadata|write|File::open|create_dir_all|"
    r"remove_file|rename|copy|exists|read_link|canonicalize|symlink_metadata)\(|"
    r"\bFile::open\(|\bthread::sleep\(|\bstd::thread::sleep\(|\.try_exists\(|\.canonicalize\(|\.read_link\(|"
    r"\b(?:Unix|Tcp)Stream::connect\(|\bto_socket_addrs\("
)
RENDER_BLOCKING = re.compile(
    r"\bCommand::new\(|\bfs::[a-z_:]+\(|\bFile::open\(|\bthread::sleep\(|\.try_exists\(|\.canonicalize\(|"
    r"\b(?:Unix|Tcp)Stream::connect\(|\bto_socket_addrs\(|"
    r"\b(?:system|runtime)::(?:run|run_with_stdin|gsettings_get|gsettings_set|which|read_to_string|read_file|"
    r"read_dir|metadata|load_persistent|load_saved)\(|\.recv\(\)|\.join\(\)|\.output\(\)|\.wait\(\)"
)
BLOCKING_ALLOWED: dict[str, str] = {
    "crates/ui/src/runtime.rs": "the guarded wrappers themselves",
    "crates/ui/src/resilience/relaunch.rs": "`exec` of a fresh copy after an unrecoverable failure: the process is being replaced",
    "crates/ui/src/files/compress.rs": "load_names reads the destination folder inside a use_resource loader, on a worker, never in render",
    "crates/ui/src/task.rs": "the worker pool: its threads sleep and spawn by design",
    "crates/ui/src/theme.rs": "reads fonts and the font family before the window exists (Application::run), never in render",
    "crates/ui/src/typography.rs": "the monospace family is asked once, at start-up, before the window exists",
    "crates/ui/src/app.rs": "the single-instance socket and its listener thread",
    "crates/ui/src/snapshot.rs": "the headless test runner, no window",
    "crates/ui/src/files/tab_tests.rs": "test fixtures: a few temporary folders written before the headless window mounts, no window thread",
    "crates/ui/src/files/place_tests.rs": "test fixtures: two temporary folders written before the headless window mounts, no window thread",
    "crates/ui/src/files/search_tests.rs": "test fixtures: a folder tree written before the headless window mounts, no window thread",
    "crates/ui/src/files/preview_step_tests.rs": "test fixtures: a folder of small files written before the headless window mounts, no window thread",
    "crates/ui/src/files/colour_tests.rs": "test: writes the tones JSON to a path the test run names (BINGUX_COLOUR_TONES), no window thread",
    "crates/ui/src/files/file_icon_surfaces.rs": 'test (cfg(all(test, feature = "testing"))): deletes the probe PNG it wrote to a path of its own, headless runner, no window thread',
    "crates/ui/src/about/details.rs": "reads os-release, the kernel release and sysfs once, in Application::run before the window exists; never in render",
    "crates/ui/src/about/link.rs": "the xdg-open child, started from a press handler and reaped on a thread of its own, and the licence-file read, run by use_resource on a worker",
    "apps/settings/src/apps.rs": "the application scanner and its loader thread; nothing in it is called from render",
    "apps/settings/src/passwd.rs": "runs `passwd` on its own thread and polls it there",
    "apps/settings/src/pages/applications.rs": "autostart file write, run inside the worker closure of the switch row",
    "apps/settings/src/pages/bluetooth.rs": "the scan runner thread (bluetoothctl, 200 ms pacing) and the rfkill/adapter reads of `load`",
    "apps/settings/src/pages/datetime/backend.rs": "the backend of `load` and the setters, called only from workers",
    "apps/settings/src/pages/keyboard/sources.rs": "the backend of the input-source loader and writer, called only from workers",
    "apps/settings/src/pages/keyboard.rs": "xdg-open child, reaped on a thread of its own",
    "apps/settings/src/pages/network/mobile.rs": "pause between the steps of a modem action, inside its worker closure",
    "apps/settings/src/pages/network/mobile_modem.rs": "pause inside the modem unlock worker closure",
    "apps/settings/src/pages/network/vpn/bus.rs": "busctl backend of the VPN loader, called only from workers",
    "apps/settings/src/pages/network/vpn/nm.rs": "nmcli backend of the VPN loader, called only from workers",
    "apps/settings/src/pages/network/vpn/wireguard.rs": "wg backend (key generation, import), called only from workers",
    "apps/settings/src/pages/network/wifi/hotspot.rs": "defaults and password generation, built inside the page's worker",
    "apps/settings/src/pages/sound.rs": "pw-dump thread and the test-tone file, both inside worker closures",
    "apps/settings/src/pages/sound/meter.rs": "the level meter's pw-record process and its reader thread",
    "apps/settings/src/pages/sound/effects/mod.rs": "the live-parameter writer's pacing sleep, on a thread of its own",
    "crates/ui/src/app_icon.rs": "the icon-theme name, asked from the icon lookup worker (guarded)",
    "crates/ui/src/files/actions/open_with.rs": "desktop-entry scan and launch, run by the file actions' workers (FilesSearch to review)",
    "crates/ui/src/files/actions/transfer.rs": "file operations of the transfer worker",
    "crates/ui/src/files/actions.rs": "file operations of the transfer worker",
    "crates/ui/src/files/address_target.rs": "metadata on a thread of its own, with a deadline",
    "crates/ui/src/files/context_menu.rs": "template listing for the New file menu: reads a folder, to move to a worker (FilesSearch)",
    "crates/ui/src/files/drives.rs": "fake unmount pause in a worker",
    "crates/ui/src/files/view_state.rs": "per-folder view state: first read and delayed write; the read is on first use, to move to a worker (FilesSearch)",
    "crates/ui/src/files/places_fixture.rs": "test fixture file, read once when a fake is requested",
    "crates/ui/src/files/reload_stop_tests.rs": "test fixtures: a folder and the pipe that stalls its read, made before the headless window mounts and removed after it",
    "crates/ui/src/files/history_ui_tests.rs": "test fixtures: a few files written before the headless window mounts, no window thread",
    "crates/ui/src/preview/behaviour_tests.rs": "test fixtures: a folder of files written before the headless window mounts, no window thread",
    "crates/ui/src/preview/render_tests.rs": "test fixtures: generated media files and the ffmpeg that makes them, written before the headless window mounts",
    "crates/ui/src/nav/slide_users_tests.rs": "test: reads the source tree to list the slide users, no window",
    "crates/ui/src/preview/host_link.rs": "the supervisor thread spawns and reaps the helper process and polls it there; the UI reads a Feed, never these calls",
}


CLOSURE = re.compile(r"\|[^|\n]*\||\|\|")


def in_closure(before: str) -> bool:
    """Whether the point is inside a closure that began earlier in `before`.

    A call inside a closure is not a call made by `render`: it runs when the
    closure does, on a worker (`commit`, `background`, `spawn_blocking`) or in
    an event handler. The first is the pattern; the second the guard in
    `bingux_ui::runtime` reports when it runs. A rule of thumb, not a parser:
    a closure is open while no bracket opened after its `|..|` has been closed
    past it.
    """
    for opener in CLOSURE.finditer(before):
        paren = brace = 0
        closed = False
        for char in before[opener.end() :]:
            if char in "([":
                paren += 1
            elif char in ")]":
                paren -= 1
            elif char == "{":
                brace += 1
            elif char == "}":
                brace -= 1
            elif char == ";" and paren <= 0 and brace <= 0:
                closed = True
            if paren < 0 or brace < 0 or closed:
                closed = True
                break
        if not closed:
            return True
    return False


def balanced_end(text: str, open_brace: int) -> int:
    depth = 0
    for index in range(open_brace, len(text)):
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return index
    return len(text)


def check_blocking(path: Path) -> list[str]:
    rel = str(path.relative_to(ROOT))
    text = strip_comments(path.read_text())
    # Unit tests may touch the disk.
    cut = min(
        (
            i
            for i in (text.find("#[cfg(test)]"), text.find("#[cfg(all(test"))
            if i != -1
        ),
        default=-1,
    )
    if cut != -1:
        text = text[:cut]
    problems = []

    def line_of(index: int) -> int:
        return text.count("\n", 0, index) + 1

    if rel not in BLOCKING_ALLOWED:
        for match in BLOCKING.finditer(text):
            problems.append(
                f"{rel}:{line_of(match.start())}: {match.group(0).rstrip('(')} blocks; use the wrappers in "
                "bingux_ui::runtime on a worker, or add the file to BLOCKING_ALLOWED in scripts/ui-lint.py with a reason"
            )
    for header in re.finditer(r"fn render\(&self[^{]*\{", text):
        end = balanced_end(text, header.end() - 1)
        body = text[header.end() : end]
        for match in RENDER_BLOCKING.finditer(body):
            if in_closure(body[: match.start()]):
                continue
            problems.append(
                f"{rel}:{line_of(header.end() + match.start())}: {match.group(0).rstrip('(')} in render blocks the "
                "UI thread; move it to a worker (bingux_ui::runtime::background / bingux_ui::task)"
            )
    # The initial value of `use_state(|| ..)` and the body of `use_memo(|| ..)`
    # run on the UI thread, in the render that first reaches them: a command, a
    # file read or a gsettings call there is as blocking as one in `render`.
    for opener in re.finditer(r"\buse_(?:state|memo)\(\s*(?:move\s*)?\|\|", text):
        open_paren = text.index("(", opener.start())
        body = args_of(text, open_paren)
        after = opener.end() - open_paren - 1
        for match in RENDER_BLOCKING.finditer(body, after):
            if in_closure(body[after : match.start()]):
                continue
            problems.append(
                f"{rel}:{line_of(open_paren + 1 + match.start())}: {match.group(0).rstrip('(')} in the initial "
                "value of a state or memo blocks the UI thread; start it on a worker "
                "(bingux_ui::runtime::background_then) and fill the state when it lands"
            )
    return problems


def blocking_problems() -> list[str]:
    found = []
    for source in (ROOT / "apps/settings/src", ROOT / "crates/ui/src"):
        for path in sorted(source.rglob("*.rs")):
            if "inspector" in path.parts or "target" in path.parts:
                continue
            found.extend(check_blocking(path))
    return found


RESILIENT_APPS = ("bingux-ui", "bingux-settings", "bingux-taxonomy", "bingux-files-app")


def panic_problems() -> list[str]:
    """Panics are never fatal: every Bingux app unwinds on panic and builds the
    patched freya-core that catches them (see "Panics are never fatal" in
    docs/bingux-ui.md). A manifest that aborts on panic, or that leaves the
    freya-core patch out, takes the error boundaries with it."""
    found = []
    for name in RESILIENT_APPS:
        manifest = crate_dir(ROOT, name) / "Cargo.toml"
        text = manifest.read_text()
        rel = manifest.relative_to(ROOT)
        if re.search(r'^\s*panic\s*=\s*"abort"', text, re.MULTILINE):
            found.append(
                f'{rel}: panic = "abort" makes every panic fatal; the error boundaries need unwinding'
            )
        for table in ("profile.dev", "profile.release"):
            section = re.search(
                rf"^\[{re.escape(table)}\]\n((?:(?!\[).*\n?)*)", text, re.MULTILINE
            )
            if not section or not re.search(
                r'^\s*panic\s*=\s*"unwind"', section.group(1), re.MULTILINE
            ):
                found.append(
                    f'{rel}: [{table}] must say panic = "unwind" (the error boundaries catch panics)'
                )
        if 'freya-core = { path = "../../vendor/freya-core" }' not in text:
            found.append(
                f"{rel}: [patch.crates-io] must patch freya-core with ../../vendor/freya-core (see vendor/README.md)"
            )
    return found


# --- Pressable things are components -----------------------------------------
#
# A glyph or a box that does something when it is pressed is a control, and
# bingux-ui has it: IconButton for an icon, the action and row buttons, rows,
# the switch, the checkbox. A rect with an `on_press` handler of Freya's own
# (`|event: Event<PressEventData>|`) gets none of that: no hover and pressed
# fill, focus ring, tooltip, accessible name or touch target. So a raw press
# handler may appear only in the modules of RAW_PRESS_ALLOWED, which are the
# controls themselves and the few surfaces that are not buttons, each with the
# reason. An app builds with the components.

RAW_PRESS = re.compile(
    r"\.on_press\(\s*(?:move\s*)?\|\s*\w+\s*:\s*Event<PressEventData>\s*\||\.on_pointer_press\(|\.on_click\("
)
RAW_PRESS_ALLOWED = {
    "crates/ui/src/popup_windows/route_tests.rs": "test fixture: a bare box whose press opens a sub-window",
    "crates/ui/src/controls/button.rs": "the button components",
    "crates/ui/src/controls/colour_picker/button.rs": "the colour button: a swatch with hover, pressed and open fills, which no library button draws",
    "crates/ui/src/controls/split_button.rs": "the split button's two parts",
    "crates/ui/src/controls/dropdown.rs": "the dropdown's trigger",
    "crates/ui/src/controls/segmented.rs": "the segmented control's segments",
    "crates/ui/src/controls/checkbox.rs": "the checkbox",
    "crates/ui/src/controls/stepper.rs": "the stepper's halves",
    "crates/ui/src/controls/action_strip.rs": "the action strip's cells",
    "crates/ui/src/controls/focus_stop.rs": "makes a keyboard stop out of any content",
    "crates/ui/src/controls/press.rs": "hit_area: a transparent touch target whose listeners do nothing and let the press bubble to the control that holds it",
    "crates/ui/src/controls/tabs.rs": "the tab itself, a selectable item, not an icon",
    "crates/ui/src/controls/tree_view.rs": "tree rows, and the expand chevron: part of the row's own keyboard model (Left and Right), not a stop of its own",
    "crates/ui/src/rows.rs": "the row",
    "crates/ui/src/shell/panel.rs": "scrims and panels that swallow presses",
    "crates/ui/src/shell/navigation.rs": "the sidebar's page rows",
    "crates/ui/src/shell/window.rs": "window buttons, excluded by design",
    "crates/ui/src/shell/overlays.rs": "the dialog and toast layers' scrims",
    "crates/ui/src/shell/banner.rs": "the notice's action button, filled in the kind's colour: the toast's action, now shared with the inline banner",
    "crates/ui/src/shell/menu.rs": "menu items",
    "crates/ui/src/window/popup.rs": "the popup's panel and scrim swallow presses",
    "crates/ui/src/motion.rs": "the focus ring's press tracking",
    "crates/ui/src/resilience/boundary.rs": "the error card must not depend on the components that failed",
    "crates/ui/src/files/search.rs": "a whole row is the button: start or stop the deep search",
    "crates/ui/src/files/address_list.rs": "the address bar's completion list rows",
    "crates/ui/src/files/address_crumbs.rs": "path crumbs, which are navigation, not icon buttons",
    "crates/ui/src/charts/flame.rs": "chart hit areas",
    "crates/ui/src/charts/view.rs": "chart hit areas: a tap on a donut's ring or legend row selects its segment (a hover does for a mouse); the segments are not controls and have no keyboard action",
}


def check_raw_press(path: Path) -> list[str]:
    rel = str(path.relative_to(ROOT))
    if rel in RAW_PRESS_ALLOWED:
        return []
    text = strip_comments(path.read_text())
    return [
        f"{rel}:{text.count(chr(10), 0, match.start()) + 1}: a press handler on a bare box; use a component "
        "(icon_button, action_button, row_button, a Row) or add the module to RAW_PRESS_ALLOWED with the reason"
        for match in RAW_PRESS.finditer(text)
    ]


def skipped(path: Path) -> bool:
    return path in SKIP_FILES or any(parent in SKIP_DIRS for parent in path.parents)


# --- Every application has an About window ------------------------------------
#
# `Application::run` registers the About window of every app, so one cannot be
# missing; what an app can forget is to say who it is. The app crate's
# `src/main.rs` therefore calls `.about(bingux_ui::app_info!(..))` on its
# `Application`: the version, licence, website and authors come from its
# Cargo.toml and the window is not the bare placeholder.


def about_problems() -> list[str]:
    found = []
    for main in own_rust_files("*/src/main.rs"):
        text = strip_comments(main.read_text())
        if "Application::new(" not in text:
            continue
        if not re.search(r"\.about\(\s*(?:bingux_ui::)?app_info!\s*\(", text):
            found.append(
                f"{main.relative_to(ROOT)}: starts an Application without `.about(bingux_ui::app_info!(..))`; "
                'every Bingux app tells its About window who it is (docs/bingux-ui.md, "About window")'
            )
    return found


# --- Every loop runs on motion::use_cycle ---------------------------------------
#
# An indeterminate bar and a spinner once looped on their own `use_animation`
# with `OnFinish::restart()`, each set up its own way. Every loop now reads
# `motion::use_cycle(ms)`, one clock kept in the scope of the component that
# draws it, so a new indicator cannot be built a different way by accident.

LOOP = re.compile(r"\bOnFinish::(?:[Rr]estart|[Rr]everse)")
LOOP_HOME = ROOT / "crates/ui/src/motion.rs"


def loop_problems() -> list[str]:
    found = []
    for source in SOURCES:
        for path in sorted(source.rglob("*.rs")):
            if "target" in path.parts or path == LOOP_HOME:
                continue
            text = strip_comments(path.read_text())
            for match in LOOP.finditer(text):
                found.append(
                    f"{path.relative_to(ROOT)}:{text.count(chr(10), 0, match.start()) + 1}: a looping animation "
                    'of its own; loop on motion::use_cycle(ms) (docs/bingux-ui.md, "Progress bars and spinners")'
                )
    return found


# ---------------------------------------------------------------------------------------------
# The shell's icon names. The shell draws only Bingux icons: shell/bingux/icons, listed in
# BinguxIconIndex.js (written by scripts/icons/build.py). Every name its QML, JavaScript and
# Python asks for, and every name the daemons send it, is a file of that set, or is mapped to one
# by BinguxIconAliases.js (the names other programs send). Nothing is left for the Adwaita theme.

SHELL_DIR = "shell/bingux"
SHELL_ICON_FILES = ("BinguxIconIndex.js", "BinguxIconAliases.js")
SHELL_EMITTERS = ("services/statusd/src", "services/searchd/src")
SYMBOLIC_NAME = re.compile(r"^[a-z0-9][a-z0-9.+-]*-symbolic$")
# Strings, or the comments that may hold a name in prose; a comment is dropped, a string is read.
SOURCE_TOKEN = re.compile(r"\"(?:\\.|[^\"\\\n])*\"|'(?:\\.|[^'\\\n])*'|`(?:\\.|[^`\\])*`|//[^\n]*|/\*.*?\*/", re.S)

# Names the shell makes from pieces, which no literal shows: the file, the text that builds
# them (the check fails when it moves, so the list cannot go stale) and the names it makes.
_POWER_PROFILES = [f"power-profile-{profile}-symbolic" for profile in ("power-saver", "balanced", "performance")]
SHELL_ICONS_DYNAMIC = (
    (
        "ControlCentreDetails.qml",
        '"network-wireless-signal-" + (',
        [f"network-wireless-signal-{level}-symbolic" for level in ("excellent", "good", "ok", "weak")],
    ),
    ("ControlCentreExtras.qml", '"power-profile-" + modelData + "-symbolic"', _POWER_PROFILES),
    ("SystemIndicators.qml", '"power-profile-" + profile + "-symbolic"', _POWER_PROFILES),
    ("SearchResult.qml", 'return "weather-clear";', ["weather-clear-symbolic"]),
    ("SearchResult.qml", 'return "accessories-calculator";', ["accessories-calculator-symbolic"]),
)


def source_strings(text: str, single_quotes: bool = True):
    """(line, string) for each string literal of `text`, comments left out."""
    for match in SOURCE_TOKEN.finditer(text):
        token = match.group(0)
        if token[0] in "\"'`" and (single_quotes or token[0] == '"'):
            yield text.count("\n", 0, match.start()) + 1, token[1:-1]


def shell_icon_problems(root: Path = ROOT, dynamic=SHELL_ICONS_DYNAMIC) -> list[str]:
    shell = root / SHELL_DIR
    index = (shell / "BinguxIconIndex.js").read_text()
    names = set(re.findall(r'^\s*"([^"]+)": true,$', index, re.M))
    aliases_text = (shell / "BinguxIconAliases.js").read_text()
    aliases = dict(re.findall(r'^\s*"([^"]+)":\s*"([^"]+)",$', aliases_text, re.M))
    found = []
    if not names:
        found.append(f"{SHELL_DIR}/BinguxIconIndex.js lists no icons; run scripts/icons/build.py")
    for name in sorted(names):
        if not (shell / "icons" / f"{name}.svg").is_file():
            found.append(f"{SHELL_DIR}/BinguxIconIndex.js lists {name}, which has no file in {SHELL_DIR}/icons")
    for name, target in sorted(aliases.items()):
        if name in names:
            found.append(f"{SHELL_DIR}/BinguxIconAliases.js maps {name}, which is a Bingux icon itself")
        if target not in names:
            found.append(f"{SHELL_DIR}/BinguxIconAliases.js maps {name} to {target}, which is not a Bingux icon")

    def check_name(where: str, name: str) -> None:
        if (aliases.get(name) or name) not in names:
            found.append(
                f"{where}: asks for the icon {name}, which the Bingux icon set does not have; "
                "draw it (scripts/icons/labels.tsv) or use one it has"
            )

    files = [
        path
        for path in sorted(shell.rglob("*"))
        if path.suffix in (".qml", ".js", ".py") and path.name not in SHELL_ICON_FILES and "icons" not in path.relative_to(shell).parts
    ]
    for path in files:
        text = path.read_text(errors="replace")
        for line, string in source_strings(text, single_quotes=path.suffix != ".rs"):
            if SYMBOLIC_NAME.match(string):
                check_name(f"{path.relative_to(root)}:{line}", string)
    for emitter in SHELL_EMITTERS:
        for path in sorted((root / emitter).glob("*.rs")):
            for line, string in source_strings(path.read_text(errors="replace"), single_quotes=False):
                if SYMBOLIC_NAME.match(string):
                    check_name(f"{path.relative_to(root)}:{line}", string)
    for filename, anchor, built in dynamic:
        matches = [path for path in files if path.name == filename and anchor in path.read_text(errors="replace")]
        if not matches:
            found.append(
                f"{SHELL_DIR}/{filename}: no longer contains {anchor}, which ui-lint's SHELL_ICONS_DYNAMIC "
                "lists as building icon names; update the list"
            )
        for name in built:
            check_name(f"{SHELL_DIR}/{filename} ({anchor})", name)
    # Bluetooth devices are named by BlueZ; the shell keeps the names it can draw.
    model = shell / "ControlCentreModel.js"
    if model.is_file():
        body = re.search(r"function bluetoothIcon\(.*?const devices = \[(.*?)\];", model.read_text(), re.S)
        if body is None:
            found.append(f"{SHELL_DIR}/ControlCentreModel.js: bluetoothIcon no longer has a `const devices = [..]` list")
        else:
            for device in re.findall(r'"([^"]+)"', body.group(1)):
                check_name(f"{SHELL_DIR}/ControlCentreModel.js (bluetoothIcon)", device + "-symbolic")
    return found


def shell_icon_self_test() -> list[str]:
    """The check on a small tree: a name the set lacks must fail, a name it has or maps must pass."""
    import tempfile

    failures = []
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        shell = root / SHELL_DIR
        (shell / "icons").mkdir(parents=True)
        (shell / "icons" / "go-next-symbolic.svg").write_text("<svg/>")
        (shell / "BinguxIconIndex.js").write_text('var names = {\n    "go-next-symbolic": true,\n};\n')
        (shell / "BinguxIconAliases.js").write_text('var names = {\n    "go-forward-symbolic": "go-next-symbolic",\n};\n')
        (shell / "Row.qml").write_text(
            'Item {\n    // "commented-symbolic" is prose\n    source: BinguxIcons.source("go-next-symbolic")\n'
            '    other: BinguxIcons.source("go-forward-symbolic")\n}\n'
        )
        if shell_icon_problems(root, dynamic=()):
            failures.append("a tree whose names the set has or maps was reported: " + "; ".join(shell_icon_problems(root, dynamic=())))
        (shell / "Row.qml").write_text('Item { source: BinguxIcons.source("bogus-icon-symbolic") }\n')
        if not any("bogus-icon-symbolic" in problem for problem in shell_icon_problems(root, dynamic=())):
            failures.append("a QML file that asks for bogus-icon-symbolic was not reported")
        (shell / "Row.qml").write_text("Item {}\n")
        (shell / "Helper.py").write_text('ICON = "bogus-helper-symbolic"\n')
        if not any("bogus-helper-symbolic" in problem for problem in shell_icon_problems(root, dynamic=())):
            failures.append("a Python helper that asks for bogus-helper-symbolic was not reported")
        (shell / "Helper.py").unlink()
        (shell / "BinguxIconAliases.js").write_text('var names = {\n    "go-forward-symbolic": "go-nowhere-symbolic",\n};\n')
        if not any("go-nowhere-symbolic" in problem for problem in shell_icon_problems(root, dynamic=())):
            failures.append("an alias to an icon the set lacks was not reported")
    return failures


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        failures = shell_icon_self_test()
        for failure in failures:
            print(f"ui-lint --self-test: {failure}")
        if not failures:
            print("ui-lint --self-test: the shell icon check fails on a name the Bingux set lacks")
        return 1 if failures else 0
    problems = []
    for source in SOURCES:
        for path in sorted(source.rglob("*.rs")):
            if "target" in path.parts or skipped(path):
                continue
            problems.extend(check(path))
            problems.extend(check_raw_press(path))
    # The controls that stand beside header buttons share their height token.
    for name, required in (
        ("controls/segmented.rs", "Dim::Control"),
        ("controls/split_button.rs", "HEADER_BUTTON"),
    ):
        text = (ROOT / "crates/ui/src" / name).read_text()
        if required not in text or "Dim::Button" in strip_comments(text):
            problems.append(
                f"{name}: height must come from {required} (the header button height), not Dim::Button"
            )
    launches = []
    for path in own_rust_files("**/*.rs"):
        if path in LAUNCH_ALLOWED:
            continue
        launches.extend(check_launch(path))
    blocking = blocking_problems()
    panics = panic_problems()
    abouts = about_problems()
    loops = loop_problems()
    icons = shell_icon_problems()
    everything = problems + launches + blocking + panics + abouts + loops + icons
    for problem in everything:
        print(problem)
    if everything:
        print(
            f"\nui-lint: {len(everything)} problem(s). Style boxes with tokens from "
            "crates/ui/src/style/; start windows with Application::run."
        )
        return 1
    print(
        "ui-lint: every box is styled with tokens, every window starts through Application::run, "
        "every app says who it is and the shell asks for no icon the Bingux set lacks."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
