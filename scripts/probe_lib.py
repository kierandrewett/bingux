"""Shared by the visual-integrity checks that read captures: contrast, popup
perimeter, structure stability, scroll continuity, number rolls.

Not a script. `scripts/pixel-check.py`, `scripts/structure-stability.py` and
`scripts/scroll-continuity.py` import it. It does four things:

  * runs the three apps' `--snapshot` captures the way `scripts/layout-lint.py`
    does (it takes that script's list of pages, popups and Files states, so a new
    page is covered by every check at once) with `BINGUX_UI_PROBE=FILE` set, so
    each capture also writes the probe of crates/ui/src/inspector/probe_checks:
    the node tree, the text nodes, the modal scopes and the scroll containers;
  * reads that probe back (`Probe`) and the capture's PNG (`pixels`);
  * measures colour the way WCAG does (`luminance`, `contrast`);
  * holds a check's allowed list, each entry with its reason, and reports an entry
    that matched nothing, so a list cannot rot (`Allow`).

Every finding carries the rule IDs of design/rules.json (`[CL-04]`).
"""

from __future__ import annotations

import argparse
import concurrent.futures
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", "/var/tmp/bingux-settings-target"))

from app_env import isolated  # noqa: E402,F401  (re-exported: the one place a capture's environment is made)

# The layout lint owns the list of pages, popups and Files states, and the fakes.
_spec = importlib.util.spec_from_file_location("layout_lint", ROOT / "scripts" / "layout-lint.py")
assert _spec and _spec.loader
lint = importlib.util.module_from_spec(_spec)
sys.modules.setdefault("layout_lint", lint)
_spec.loader.exec_module(lint)

DESKTOP = lint.DESKTOP
PHONE = lint.PHONE

# Node flags of the probe (probe_checks/mod.rs).
VISIBLE, PAINTS, TEXT, MODAL, SCROLLER, WRAPPER, OUT_OF_FLOW, INERT, CLIP = 1, 2, 4, 8, 16, 32, 64, 128, 256

SCHEMES = {"dark": "prefer-dark", "light": "prefer-light"}


# ---- the probe ---------------------------------------------------------------


@dataclass
class Probe:
    """One capture: the PNG and the facts about its tree."""

    capture: str
    scale: float
    window: tuple[float, float]
    titles: list[str]
    strings: list[str]
    nodes: list[tuple]
    texts: list[list]
    modals: list[dict]
    scrollers: list[dict]
    job: str = ""
    variant: str = ""

    @classmethod
    def from_json(cls, data: dict, job: str = "", variant: str = "") -> "Probe":
        return cls(
            capture=data["capture"],
            scale=float(data["scale"]),
            window=(float(data["window"][0]), float(data["window"][1])),
            titles=data["titles"],
            strings=data["strings"],
            nodes=[tuple(node) for node in data["nodes"]],
            texts=data["texts"],
            modals=data["modals"],
            scrollers=data["scrollers"],
            job=job,
            variant=variant,
        )

    @property
    def name(self) -> str:
        return f"{self.job}:{Path(self.capture).stem}" + (f"@{self.variant}" if self.variant else "")

    def box(self, index: int) -> tuple[float, float, float, float]:
        node = self.nodes[index]
        return node[2], node[3], node[4], node[5]

    def title(self, index: int) -> str:
        return self.titles[self.nodes[index][1]]

    def flags(self, index: int) -> int:
        return self.nodes[index][6]

    def parent(self, index: int) -> int:
        return self.nodes[index][0]

    def text(self, index: int) -> str:
        at = self.nodes[index][7]
        return self.strings[at] if at >= 0 else ""

    def path(self, index: int) -> str:
        """Component path from the root, nearest last, plain boxes left out."""
        parts: list[str] = []
        while index >= 0:
            title = self.title(index)
            if title not in ("row", "column", "element", "text"):
                parts.append(title)
            index = self.parent(index)
        return "/".join(reversed(parts[:6])) if parts else ""

    def ancestors(self, index: int):
        index = self.parent(index)
        while index >= 0:
            yield index
            index = self.parent(index)

    def children(self) -> dict[int, list[int]]:
        kids: dict[int, list[int]] = {}
        for at, node in enumerate(self.nodes):
            kids.setdefault(node[0], []).append(at)
        return kids

    def pixels(self) -> np.ndarray:
        """The capture as an H x W x 3 array of 0..255 floats."""
        with Image.open(self.capture) as image:
            return np.asarray(image.convert("RGB"), dtype=np.float64)


def read_probes(path: Path, job: str = "", variant: str = "") -> list[Probe]:
    if not path.exists():
        return []
    probes = []
    for line in path.read_text().splitlines():
        if line.strip():
            probes.append(Probe.from_json(json.loads(line), job, variant))
    return probes


# ---- colour ------------------------------------------------------------------


def to_linear(channel: np.ndarray) -> np.ndarray:
    """sRGB 0..255 to linear light, in chunks.

    numpy 2.2.6 on this machine returns wrong values from the vectorised power for arrays over
    about 30000 elements (a pixel of 245 read 0.0065 instead of 0.91, in a text line's crop), so the
    array is done in pieces of 8192 elements, each of which is exact.
    """
    flat = np.asarray(channel, dtype=np.float64).reshape(-1)
    out = np.empty_like(flat)
    for start in range(0, flat.size, 8192):
        value = flat[start : start + 8192] / 255.0
        out[start : start + 8192] = np.where(value <= 0.04045, value / 12.92, ((value + 0.055) / 1.055) ** 2.4)
    return out.reshape(np.shape(channel))


def luminance(rgb) -> np.ndarray:
    """WCAG relative luminance of RGB values in 0..255; the last axis is the channel."""
    linear = to_linear(np.asarray(rgb))
    return 0.2126 * linear[..., 0] + 0.7152 * linear[..., 1] + 0.0722 * linear[..., 2]


def contrast(a, b) -> float:
    """WCAG contrast ratio of two relative luminances."""
    hi, lo = max(float(a), float(b)), min(float(a), float(b))
    return (hi + 0.05) / (lo + 0.05)


def over(fg, alpha: float, bg) -> np.ndarray:
    """`fg` at `alpha` painted over `bg`, both RGB in 0..255."""
    return np.asarray(fg, dtype=np.float64) * alpha + np.asarray(bg, dtype=np.float64) * (1.0 - alpha)


# ---- running captures ----------------------------------------------------------


@dataclass
class Variant:
    scheme: str = "dark"
    scale: int = 1

    @property
    def tag(self) -> str:
        return f"{self.scheme}{'' if self.scale == 1 else f'x{self.scale}'}"


def parse_variants(text: str) -> list[Variant]:
    """`dark,light,darkx2` to variants: a scheme, then `x` and the scale."""
    found = []
    for part in text.split(","):
        part = part.strip()
        if not part:
            continue
        scheme, _, scale = part.partition("x")
        if scheme not in SCHEMES:
            sys.exit(f"unknown scheme {scheme!r} in --variants (dark, light, darkx2 ...)")
        found.append(Variant(scheme, int(scale) if scale else 1))
    return found


def variant_env(variant: Variant) -> dict[str, str]:
    return {
        "BINGUX_SETTINGS_SNAPSHOT_SCHEME": SCHEMES[variant.scheme],
        "BINGUX_TAXONOMY_THEME": variant.scheme,
        "BINGUX_SNAPSHOT_SCALE": str(variant.scale),
    }


def run_job(job: dict, workdir: Path, timeout: int, variant: Variant, extra: dict[str, str] | None = None) -> dict:
    """Run one capture job with the probe on. Returns {name, status, probes, log}."""
    name = f"{job['name']}.{variant.tag}"
    probe_file = workdir / "probe" / f"{name}.jsonl"
    probe_file.parent.mkdir(parents=True, exist_ok=True)
    probe_file.unlink(missing_ok=True)
    env = dict(os.environ)
    env.update(lint.FAKES)
    env.update(job.get("env", {}))
    env.update(variant_env(variant))
    env.update(extra or {})
    env["BINGUX_UI_PROBE"] = str(probe_file)
    # A capture is rendered in software with no compositor: it must never find the live desktop,
    # and its Settings pages must never write the person's dconf (scripts/app_env.py).
    env = isolated(env)
    env["XDG_CACHE_HOME"] = str(workdir / "cache" / name)
    if job.get("size") and job["size"] != DESKTOP:
        env["BINGUX_SETTINGS_SNAPSHOT_SIZE"] = job["size"]
    if job.get("script"):
        env["BINGUX_SETTINGS_SNAPSHOT_SCRIPT"] = re.sub(r"save popup(?![\w.-])", "save popup.png", job["script"])
    env.setdefault("BINGUX_SETTINGS_SNAPSHOT_SETTLE_MS", "1500")
    out = workdir / "png" / name
    out.mkdir(parents=True, exist_ok=True)
    log = workdir / "logs" / f"{name}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as sink:
        try:
            proc = subprocess.run(
                [*job["argv"], str(out), *job.get("pages", [])],
                env=env,
                cwd=out,
                stdout=sink,
                stderr=subprocess.STDOUT,
                timeout=timeout,
                check=False,
            )
            status = "ok" if proc.returncode == 0 else f"exit {proc.returncode}"
        except subprocess.TimeoutExpired:
            status = f"timed out after {timeout}s"
    probes = read_probes(probe_file, job["name"], variant.tag)
    for probe in probes:
        # A script's `save` names a file relative to the directory the run was started in.
        if not Path(probe.capture).is_absolute():
            probe.capture = str(out / probe.capture)
    return {"name": name, "status": status, "probes": probes, "log": str(log)}


def add_sweep_args(parser: argparse.ArgumentParser, variants: str = "dark,light") -> None:
    parser.add_argument("--only", choices=["settings", "taxonomy", "files"], help="one app's captures")
    parser.add_argument("--pages", default="", help="comma-separated Settings pages (default: every page and subpage)")
    parser.add_argument("--sizes", default=f"{DESKTOP},{PHONE}", help="Settings sizes, `default` is desktop")
    parser.add_argument("--variants", default=variants, help="schemes and scales: dark, light, darkx2, lightx2")
    parser.add_argument("--jobs", type=int, default=2, help="captures to run at once")
    parser.add_argument("--timeout", type=int, default=240, help="seconds per capture run")
    parser.add_argument("--keep", help="keep captures and logs in this directory")
    parser.add_argument("--build", action="store_true", help="build the three binaries first")
    parser.add_argument("--json", help="write every finding to this file")
    parser.add_argument("--show-allowed", action="store_true", help="list the findings the allowed list covers")


def taxonomy_pages() -> list[str]:
    """`category/page` of every taxonomy page, from `bingux-taxonomy --help`."""
    out = subprocess.run([str(TARGET / "debug" / "bingux-taxonomy"), "--help"], capture_output=True, text=True, check=False).stdout
    return re.findall(r"^  ([a-z0-9-]+/[a-z0-9-]+)$", out, re.M)


# Pages whose capture at a render scale above 1 panics inside the cargo-registry crate freya-testing
# (a `None` unwrapped in its accessibility update, freya-testing-0.5.0-rc.10/src/lib.rs:685) after the
# page is drawn: a fault of the harness at 2x that this repository cannot patch (the vendored
# freya-core's two unwraps that also stopped it at 2x are fixed). The page is left out of the runs at
# a scale above 1.
TAXONOMY_SKIP_AT_SCALE = {"files/address-bar-and-places", "files/file-manager-and-chooser"}


def sweep_jobs(args: argparse.Namespace, workdir: Path, overlays_only: bool, scale: int = 1) -> list[dict]:
    """The capture jobs of layout-lint, for the apps `--only` selects.

    `overlays_only` leaves out the plain Settings pages and the Files folder list:
    the popups, dialogs and menus (and the taxonomy, which holds the menus and
    dialogs of every component) are what a check of overlays wants.
    """
    settings = TARGET / "debug" / "bingux-settings"
    taxonomy = TARGET / "debug" / "bingux-taxonomy"
    files = TARGET / "debug" / "bingux-files"
    jobs: list[dict] = []
    if args.only in (None, "settings"):
        if not settings.exists():
            sys.exit(f"{settings} is missing: build with --build")
        pages = [page for page in args.pages.split(",") if page] or lint.panels() + lint.SUBPAGES
        sizes = [size for size in args.sizes.split(",") if size]
        for job in lint.settings_jobs(pages, sizes):
            if overlays_only and job["name"].startswith("settings-"):
                continue
            jobs.append(job)
    if args.only in (None, "taxonomy") and not args.pages:
        if not taxonomy.exists():
            sys.exit(f"{taxonomy} is missing: build with --build")
        if scale > 1:
            # One run a page, so that a page that stops the harness does not take the rest with it.
            taxonomy = str(TARGET / "debug" / "bingux-taxonomy")
            jobs += [
                {"name": f"taxonomy-{page.replace('/', '_')}", "argv": [taxonomy, "--snapshot"], "pages": [page]}
                for page in taxonomy_pages()
                if page not in TAXONOMY_SKIP_AT_SCALE
            ]
        else:
            jobs += lint.taxonomy_jobs()
    if args.only in (None, "files") and not args.pages:
        if not files.exists():
            sys.exit(f"{files} is missing: build with --build")
        for job in lint.files_jobs(workdir):
            if overlays_only and job["name"].startswith("files-list-"):
                continue
            if scale > 1 and re.match(r"files-properties-.*-phone$", job["name"]):
                continue  # the same fault of freya-testing at 2x as TAXONOMY_SKIP_AT_SCALE (its lib.rs:685, again)
            jobs.append(job)
    return jobs


def run_sweep(args: argparse.Namespace, workdir: Path, variants: list[Variant], overlays_at_scale: bool = True) -> list[dict]:
    """Every job at every variant. At a scale above 1 only the overlays are run."""
    results: list[dict] = []
    plan: list[tuple[dict, Variant]] = []
    for variant in variants:
        for job in sweep_jobs(args, workdir, overlays_only=variant.scale > 1 and overlays_at_scale, scale=variant.scale):
            plan.append((job, variant))
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futures = [
            pool.submit(run_job, job, workdir, args.timeout * (30 if job["name"] == "taxonomy" else 1), variant)
            for job, variant in plan
        ]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            print(f"{result['status']:>10}  {result['name']}  ({len(result['probes'])} captures)", flush=True)
    return results


def workdir_of(args: argparse.Namespace, prefix: str) -> Path:
    if args.keep:
        path = Path(args.keep)
        path.mkdir(parents=True, exist_ok=True)
        return path
    return Path(tempfile.mkdtemp(prefix=prefix))


# ---- findings and allowed lists --------------------------------------------------


@dataclass
class Finding:
    rule: str  # the check's rule key, `popup-perimeter`
    ids: list[str]  # design/rules.json IDs, `["BR-03"]`
    capture: str
    target: str
    what: str
    numbers: dict[str, float] = field(default_factory=dict)
    allowed: str | None = None

    def json(self) -> dict:
        return {
            "rule": self.rule,
            "ids": self.ids,
            "capture": self.capture,
            "target": self.target,
            "what": self.what,
            "numbers": self.numbers,
            "allowed": self.allowed,
        }


class Allow:
    """Findings that are right by design: rule key, capture regex, target regex, reason."""

    def __init__(self, entries: list[dict]):
        self.entries = entries
        self.used: set[int] = set()

    def why(self, finding: Finding) -> str | None:
        for at, entry in enumerate(self.entries):
            if (
                re.fullmatch(entry["rule"], finding.rule)
                and re.search(entry["capture"], finding.capture)
                and re.search(entry["where"], finding.target)
            ):
                self.used.add(at)
                return entry["why"]
        return None

    def unused(self) -> list[dict]:
        return [entry for at, entry in enumerate(self.entries) if at not in self.used]


def finish(
    name: str,
    findings: list[Finding],
    allow: Allow,
    args: argparse.Namespace,
    results: list[dict],
    measured: int,
    workdir: Path,
    measured_what: str,
) -> int:
    """Print the findings, write --json, tidy up and return the exit status."""
    for finding in findings:
        finding.allowed = allow.why(finding)
    failures = [finding for finding in findings if not finding.allowed]
    broken = [result for result in results if result["status"] != "ok" or not result["probes"]]
    print()
    for finding in sorted(failures, key=lambda f: (f.rule, f.capture, f.target)):
        numbers = ", ".join(f"{key} {value:g}" for key, value in finding.numbers.items())
        print(f"[{finding.rule}] {'/'.join(finding.ids)} {finding.capture}")
        print(f"    {finding.target}: {finding.what}" + (f"  ({numbers})" if numbers else ""))
    if getattr(args, "show_allowed", False):
        for finding in findings:
            if finding.allowed:
                print(f"[allowed {finding.rule}] {finding.capture}: {finding.target} -- {finding.allowed}")
    for entry in allow.unused():
        print(f"unused allowed entry: {entry['rule']} {entry['capture']} {entry['where']}: {entry['why']}")
    for result in broken:
        print(f"no captures: {result['name']} ({result['status']}), see {result['log']}")
    if getattr(args, "json", None):
        Path(args.json).write_text(json.dumps([finding.json() for finding in findings], indent=2))
    by_rule = Counter(finding.rule for finding in failures)
    print(
        f"\n{name}: {len(results)} runs, {measured} {measured_what}, {len(failures)} findings"
        + (": " + ", ".join(f"{rule} {count}" for rule, count in sorted(by_rule.items())) if by_rule else "")
        + f", {len(findings) - len(failures)} allowed"
    )
    empty = measured == 0
    if empty:
        print(f"{name}: nothing was measured, so nothing passed")
    if not getattr(args, "keep", None):
        shutil.rmtree(workdir, ignore_errors=True)
    return 1 if failures or broken or allow.unused() or empty else 0
