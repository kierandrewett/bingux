#!/usr/bin/env python3
"""Fail when Bingux UI code styles a box outside the token system.

Every spacing, radius, size, colour, line, shadow and text style comes from
packages/bingux-ui/src/style/ (tokens.rs and recipes.rs) and
packages/bingux-ui/src/typography.rs. Boxes are styled with `.tw(&tokens,
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
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES = [
    ROOT / "packages/bingux-ui/src",
    ROOT / "packages/bingux-ui/examples",
    ROOT / "packages/bingux-settings/src",
    ROOT / "packages/bingux-files-app/src",
    ROOT / "packages/bingux-taxonomy/src",
]
# The style system defines the values; the palette and icon table hold raw
# colours and SVG; the snapshot runner and the icon contact-sheet example are
# test tooling (they need exact pixel sizes and plain black and white).
SKIP_DIRS = {ROOT / "packages/bingux-ui/src/style"}
SKIP_FILES = {
    ROOT / "packages/bingux-ui/src/theme.rs",
    ROOT / "packages/bingux-ui/src/typography.rs",
    ROOT / "packages/bingux-ui/src/icons.rs",
    ROOT / "packages/bingux-ui/src/snapshot.rs",
    ROOT / "packages/bingux-ui/examples/icon_sheet.rs",
    ROOT / "packages/bingux-ui/src/surface.rs",
    # Contrast measurement: colour maths and the matrix that reads raw tokens.
    ROOT / "packages/bingux-ui/src/contrast.rs",
    ROOT / "packages/bingux-ui/src/contrast_matrix.rs",
    ROOT / "packages/bingux-ui/src/charts/palette.rs",
    ROOT / "packages/bingux-ui/src/charts/svg.rs",
}

STYLING = re.compile(
    r"\.(padding|spacing|margin|corner_radius|rounded(?:_[a-z0-9]+)?|background|border|shadow|"
    r"font_size|font_weight|color|direction|content|cross_align|main_align|overflow)\(|\.center\(\)"
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
        report(match.start(), f"{match.group(0).rstrip('(')}(..) styles a box directly; use .tw(&tokens, ..)")
    for match in COLOUR_FIELD.finditer(text):
        report(match.start(), f"{match.group(0)} is a raw colour; use Paint::.. or Ink::..")
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
        numbers = [m.group(1) for m in LITERAL.finditer(args) if m.group(1) not in ("0", "0.0")]
        if numbers or ".px()" in args:
            report(
                match.start(), f".{match.group(1)}({args.strip()[:40]}) uses a fixed size; use w(Dim::..)/h(Dim::..)"
            )
    # A header's icon buttons take their size from HEADER_BUTTON (the control
    # height) inside HeaderBar; sizing one in a header drifts the bar.
    for match in re.finditer(r"\.(leading|trailing)\(", text):
        if re.search(r"\.size\(", args_of(text, match.end() - 1)):
            report(match.start(), f".{match.group(1)}(..) sizes a header element; header buttons use HEADER_BUTTON")
    for match in ICON.finditer(text):
        parts = [part.strip() for part in args_of(text, match.end() - 1).split(",")]
        if len(parts) > 1 and not parts[1].startswith(("IconSize::", "size", "self.size")):
            if LITERAL.search(parts[1]):
                report(match.start(), f"icon size {parts[1]} is not an IconSize")
    return problems


# The only files that may touch Freya's launch API: the app loop that builds the
# one launch configuration, and the font helper it calls.
LAUNCH_ALLOWED = {
    ROOT / "packages/bingux-ui/src/app.rs",
    ROOT / "packages/bingux-ui/src/theme.rs",
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
# through State. Two rules, for packages/bingux-settings/src and
# packages/bingux-ui/src (not the inspector, which is developer tooling):
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
    "packages/bingux-ui/src/runtime.rs": "the guarded wrappers themselves",
    "packages/bingux-ui/src/resilience/relaunch.rs": "`exec` of a fresh copy after an unrecoverable failure: the process is being replaced",
    "packages/bingux-ui/src/files/compress.rs": "load_names reads the destination folder inside a use_resource loader, on a worker, never in render",
    "packages/bingux-ui/src/task.rs": "the worker pool: its threads sleep and spawn by design",
    "packages/bingux-ui/src/theme.rs": "reads fonts and the font family before the window exists (Application::run), never in render",
    "packages/bingux-ui/src/typography.rs": "the monospace family is asked once, at start-up, before the window exists",
    "packages/bingux-ui/src/app.rs": "the single-instance socket and its listener thread",
    "packages/bingux-ui/src/snapshot.rs": "the headless test runner, no window",
    "packages/bingux-ui/src/about/details.rs": "reads os-release, the kernel release and sysfs once, in Application::run before the window exists; never in render",
    "packages/bingux-ui/src/about/link.rs": "the xdg-open child, started from a press handler and reaped on a thread of its own, and the licence-file read, run by use_resource on a worker",
    "packages/bingux-settings/src/apps.rs": "the application scanner and its loader thread; nothing in it is called from render",
    "packages/bingux-settings/src/passwd.rs": "runs `passwd` on its own thread and polls it there",
    "packages/bingux-settings/src/pages/applications.rs": "autostart file write, run inside the worker closure of the switch row",
    "packages/bingux-settings/src/pages/bluetooth.rs": "the scan runner thread (bluetoothctl, 200 ms pacing) and the rfkill/adapter reads of `load`",
    "packages/bingux-settings/src/pages/datetime/backend.rs": "the backend of `load` and the setters, called only from workers",
    "packages/bingux-settings/src/pages/keyboard/sources.rs": "the backend of the input-source loader and writer, called only from workers",
    "packages/bingux-settings/src/pages/keyboard.rs": "xdg-open child, reaped on a thread of its own",
    "packages/bingux-settings/src/pages/network/mobile.rs": "pause between the steps of a modem action, inside its worker closure",
    "packages/bingux-settings/src/pages/network/mobile_modem.rs": "pause inside the modem unlock worker closure",
    "packages/bingux-settings/src/pages/network/vpn/bus.rs": "busctl backend of the VPN loader, called only from workers",
    "packages/bingux-settings/src/pages/network/vpn/nm.rs": "nmcli backend of the VPN loader, called only from workers",
    "packages/bingux-settings/src/pages/network/vpn/wireguard.rs": "wg backend (key generation, import), called only from workers",
    "packages/bingux-settings/src/pages/network/wifi/hotspot.rs": "defaults and password generation, built inside the page's worker",
    "packages/bingux-settings/src/pages/sound.rs": "pw-dump thread and the test-tone file, both inside worker closures",
    "packages/bingux-settings/src/pages/sound/meter.rs": "the level meter's pw-record process and its reader thread",
    "packages/bingux-settings/src/pages/sound/effects/mod.rs": "the live-parameter writer's pacing sleep, on a thread of its own",
    "packages/bingux-ui/src/app_icon.rs": "the icon-theme name, asked from the icon lookup worker (guarded)",
    "packages/bingux-ui/src/files/actions/open_with.rs": "desktop-entry scan and launch, run by the file actions' workers (FilesSearch to review)",
    "packages/bingux-ui/src/files/actions/transfer.rs": "file operations of the transfer worker",
    "packages/bingux-ui/src/files/actions.rs": "file operations of the transfer worker",
    "packages/bingux-ui/src/files/address_target.rs": "metadata on a thread of its own, with a deadline",
    "packages/bingux-ui/src/files/context_menu.rs": "template listing for the New file menu: reads a folder, to move to a worker (FilesSearch)",
    "packages/bingux-ui/src/files/drives.rs": "fake unmount pause in a worker",
    "packages/bingux-ui/src/files/view_state.rs": "per-folder view state: first read and delayed write; the read is on first use, to move to a worker (FilesSearch)",
    "packages/bingux-ui/src/files/places_fixture.rs": "test fixture file, read once when a fake is requested",
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
    cut = text.find("#[cfg(test)]")
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
    for source in (ROOT / "packages/bingux-settings/src", ROOT / "packages/bingux-ui/src"):
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
        manifest = ROOT / "packages" / name / "Cargo.toml"
        text = manifest.read_text()
        rel = manifest.relative_to(ROOT)
        if re.search(r'^\s*panic\s*=\s*"abort"', text, re.MULTILINE):
            found.append(f'{rel}: panic = "abort" makes every panic fatal; the error boundaries need unwinding')
        for table in ("profile.dev", "profile.release"):
            section = re.search(rf"^\[{re.escape(table)}\]\n((?:(?!\[).*\n?)*)", text, re.MULTILINE)
            if not section or not re.search(r'^\s*panic\s*=\s*"unwind"', section.group(1), re.MULTILINE):
                found.append(f'{rel}: [{table}] must say panic = "unwind" (the error boundaries catch panics)')
        if 'freya-core = { path = "../vendor/freya-core" }' not in text:
            found.append(f"{rel}: [patch.crates-io] must patch freya-core with ../vendor/freya-core (see packages/vendor/README.md)")
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
    "packages/bingux-ui/src/controls/button.rs": "the button components",
    "packages/bingux-ui/src/controls/split_button.rs": "the split button's two parts",
    "packages/bingux-ui/src/controls/dropdown.rs": "the dropdown's trigger",
    "packages/bingux-ui/src/controls/segmented.rs": "the segmented control's segments",
    "packages/bingux-ui/src/controls/checkbox.rs": "the checkbox",
    "packages/bingux-ui/src/controls/stepper.rs": "the stepper's halves",
    "packages/bingux-ui/src/controls/action_strip.rs": "the action strip's cells",
    "packages/bingux-ui/src/controls/focus_stop.rs": "makes a keyboard stop out of any content",
    "packages/bingux-ui/src/controls/tabs.rs": "the tab itself, a selectable item, not an icon",
    "packages/bingux-ui/src/controls/tree_view.rs": "tree rows, and the expand chevron: part of the row's own keyboard model (Left and Right), not a stop of its own",
    "packages/bingux-ui/src/rows.rs": "the row",
    "packages/bingux-ui/src/shell/panel.rs": "scrims and panels that swallow presses",
    "packages/bingux-ui/src/shell/navigation.rs": "the sidebar's page rows",
    "packages/bingux-ui/src/shell/window.rs": "window buttons, excluded by design",
    "packages/bingux-ui/src/shell/overlays.rs": "the dialog and toast layers' scrims",
    "packages/bingux-ui/src/shell/banner.rs": "the notice's action button, filled in the kind's colour: the toast's action, now shared with the inline banner",
    "packages/bingux-ui/src/shell/menu.rs": "menu items",
    "packages/bingux-ui/src/window/popup.rs": "the popup's panel and scrim swallow presses",
    "packages/bingux-ui/src/motion.rs": "the focus ring's press tracking",
    "packages/bingux-ui/src/resilience/boundary.rs": "the error card must not depend on the components that failed",
    "packages/bingux-ui/src/files/search.rs": "a whole row is the button: start or stop the deep search",
    "packages/bingux-ui/src/files/address_list.rs": "the address bar's completion list rows",
    "packages/bingux-ui/src/files/address_crumbs.rs": "path crumbs, which are navigation, not icon buttons",
    "packages/bingux-ui/src/charts/flame.rs": "chart hit areas",
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
    for main in sorted((ROOT / "packages").glob("*/src/main.rs")):
        relative = main.relative_to(ROOT / "packages").parts
        if relative[0] == "vendor":
            continue
        text = strip_comments(main.read_text())
        if "Application::new(" not in text:
            continue
        if not re.search(r"\.about\(\s*(?:bingux_ui::)?app_info!\s*\(", text):
            found.append(
                f"{main.relative_to(ROOT)}: starts an Application without `.about(bingux_ui::app_info!(..))`; "
                "every Bingux app tells its About window who it is (docs/bingux-ui.md, \"About window\")"
            )
    return found


def main() -> int:
    problems = []
    for source in SOURCES:
        for path in sorted(source.rglob("*.rs")):
            if "target" in path.parts or skipped(path):
                continue
            problems.extend(check(path))
            problems.extend(check_raw_press(path))
    # The controls that stand beside header buttons share their height token.
    for name, required in (("controls/segmented.rs", "Dim::Control"), ("controls/split_button.rs", "HEADER_BUTTON")):
        text = (ROOT / "packages/bingux-ui/src" / name).read_text()
        if required not in text or "Dim::Button" in strip_comments(text):
            problems.append(f"{name}: height must come from {required} (the header button height), not Dim::Button")
    launches = []
    for path in sorted((ROOT / "packages").rglob("*.rs")):
        relative = path.relative_to(ROOT / "packages").parts
        if relative[0] == "vendor" or "target" in relative or path in LAUNCH_ALLOWED:
            continue
        launches.extend(check_launch(path))
    blocking = blocking_problems()
    panics = panic_problems()
    abouts = about_problems()
    everything = problems + launches + blocking + panics + abouts
    for problem in everything:
        print(problem)
    if everything:
        print(
            f"\nui-lint: {len(everything)} problem(s). Style boxes with tokens from "
            "packages/bingux-ui/src/style/; start windows with Application::run."
        )
        return 1
    print(
        "ui-lint: every box is styled with tokens, every window starts through Application::run "
        "and every app says who it is."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
