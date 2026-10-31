#!/usr/bin/env python3
"""List the licences of the Rust crates linked into each Bingux program.

The Freya applications link their Rust dependencies statically, so each
package must carry those licences and their copyright notices. For every
program this writes `OUT/<program>.LICENSE.dependencies`: one line per crate
with its SPDX expression, then the text of every licence file the crates ship
(a text shared by several crates is printed once, with the list of crates). It
also prints the SPDX expression for the program's `License:` tag: the unique
licence expressions of all linked crates, joined with AND. The packages install
the files with `%license`.

usage: scripts/cargo-licenses.py [--out DIR] [--summary] [--check]

  --out DIR   write the per-program files there (default: build/licenses)
  --summary   print `program: expression` for every program
  --check     compare the expression with the License tag of each package in
              packaging/rpm/bingux.spec and exit 1 when they differ
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from crate_dirs import crate_dir  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]

# program name -> crate directory holding its Cargo.toml.
PROGRAMS = {
    "bingux-settings": "bingux-settings",
    "bingux-files": "bingux-files-app",
    "bingux-taxonomy": "bingux-taxonomy",
    "bingux-preview-host": "bingux-preview-host",
    "bingux-config": "bingux-config",
    "bingux-searchd": "bingux-searchd",
    "bingux-statusd": "bingux-statusd",
    "bingux-effects": "bingux-audio-effects",
    "bingux-dsp": "bingux-dsp",
}


# The RPM package that ships each program, and the licences that package adds
# for files that are not Rust crates (Quickshell, which the shell binary embeds, is
# LGPL; the project's own code is GPL).
PACKAGE_OF = {
    "bingux-settings": "bingux-settings",
    "bingux-files": "bingux-files",
    "bingux-taxonomy": "bingux-taxonomy",
    "bingux-preview-host": "bingux-preview-host",
    "bingux-config": "bingux-config",
    "bingux-searchd": "bingux",
    "bingux-statusd": "bingux",
    "bingux-effects": "bingux-audio-effects",
    "bingux-dsp": "bingux-audio-effects",
}
OWN = ["GPL-3.0-or-later"]
EXTRA = {"bingux": ["LGPL-3.0-or-later"]}


def normalise(expression):
    """One spelling for the same licence choice, without cargo's markers."""
    expression = re.sub(r"\s*\((?:\*|proc-macro)\)", "", expression).strip()
    expression = re.sub(r"\s*/\s*", " OR ", expression)
    if " AND " not in expression and "(" not in expression and " WITH " not in expression:
        expression = " OR ".join(sorted(set(expression.split(" OR "))))
    return expression


def crates(crate_name):
    """(name, version, licence) of every crate in the normal dependency graph."""
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "--manifest-path",
            str(crate_dir(ROOT, crate_name) / "Cargo.toml"),
            "--locked",
            "-e",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}|{l}",
        ],
        capture_output=True,
        text=True,
        check=False,
        env={**__import__("os").environ, "CARGO_NET_OFFLINE": "true"},
    )
    if result.returncode != 0:
        raise SystemExit(f"cargo tree failed for {crate_name}:\n{result.stderr}")
    found, paths = {}, {}
    for line in result.stdout.splitlines():
        package, _, licence = line.partition("|")
        package = re.sub(r"\s*\((?:\*|proc-macro)\)", "", package)
        # "name vX.Y.Z (/path/to/crate)" for path dependencies.
        name, version = package.split()[:2]
        key = (name, version.lstrip("v"))
        if match := re.search(r"\((/[^)]*)\)", package):
            paths[key] = match.group(1)
        found[key] = normalise(licence) or "UNKNOWN"
    return [(name, version, licence) for (name, version), licence in sorted(found.items())], paths


LICENSE_FILE = re.compile(r"^(licen[sc]e|copying|unlicense)([-_.].*)?$", re.I)


def crate_root(name, version, path):
    """The source directory of a crate: its path, or the unpacked registry copy."""
    if path:
        return Path(path)
    home = Path(os.environ.get("CARGO_HOME") or Path.home() / ".cargo")
    for candidate in sorted((home / "registry/src").glob(f"*/{name}-{version}")):
        return candidate
    return None


def licence_texts(found, paths):
    """{text: [crate names]} of every licence file the crates ship."""
    texts = {}
    for name, version, _ in found:
        root = crate_root(name, version, paths.get((name, version)))
        if root is None or not root.is_dir():
            continue
        for path in sorted(root.iterdir()):
            if path.is_file() and LICENSE_FILE.match(path.name) and path.stat().st_size < 200_000:
                text = path.read_text(errors="replace").strip()
                if text:
                    texts.setdefault(text, []).append(name)
    return texts


def split_top_level(expression, separator):
    """Split on `separator` outside parentheses."""
    parts, depth, start = [], 0, 0
    for index, char in enumerate(expression):
        depth += (char == "(") - (char == ")")
        if depth == 0 and expression.startswith(separator, index):
            parts.append(expression[start:index])
            start = index + len(separator)
    parts.append(expression[start:])
    return [part.strip() for part in parts]


def terms(expression):
    """The AND terms of an expression, each a plain licence or an OR choice."""
    result = []
    for part in split_top_level(expression, " AND "):
        if part.startswith("(") and part.endswith(")") and len(split_top_level(part[1:-1], " AND ")) > 1:
            result += terms(part[1:-1])
        else:
            result.append(part[1:-1] if part.startswith("(") and part.endswith(")") else part)
    return result


def expression(licences):
    """The licence expression of the combined work: every licence that applies, joined with AND.

    A crate that offers a choice (`MIT OR Apache-2.0`) is taken under a licence the
    combined work carries anyway when one of its choices is already required by
    another crate; a choice that has no such member stays as an OR term.
    """
    atoms = {term for licence in licences for term in terms(licence)}
    plain = {atom for atom in atoms if " OR " not in atom}
    kept = {
        atom
        for atom in atoms
        if atom in plain or not any(option.strip() in plain for option in atom.split(" OR "))
    }
    wrap = len(kept) > 1
    return " AND ".join(f"({atom})" if wrap and " OR " in atom else atom for atom in sorted(kept))


def spec_licences():
    """License tag of each package in the spec: {package name: expression}."""
    text = (ROOT / "packaging/rpm/bingux.spec").read_text()
    tags = {}
    name = None
    for line in text.splitlines():
        if match := re.match(r"%package\s+-n\s+(\S+)", line):
            name = match.group(1)
        elif match := re.match(r"%package\s+(\S+)", line):
            name = "bingux-" + match.group(1)
        elif line.startswith("Name:"):
            name = line.split()[1]
        elif match := re.match(r"License:\s*(.+)", line):
            tags[name] = match.group(1).strip()
    return tags


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", default="build/licenses")
    parser.add_argument("--summary", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    summaries = {}
    for program, crate_name in PROGRAMS.items():
        found, paths = crates(crate_name)
        licences = [licence for name, _, licence in found if not name.startswith("bingux-")]
        summary = expression(licences)
        summaries[program] = summary
        lines = [
            f"Licences of the Rust crates linked into {program}",
            "(generated by scripts/cargo-licenses.py from the crate metadata in Cargo.lock)",
            "",
            f"Summary: {summary}",
            "",
        ]
        external = [(name, version, licence) for name, version, licence in found if not name.startswith("bingux-")]
        lines += [f"{name} {version}: {licence}" for name, version, licence in external]
        texts = licence_texts(external, paths)
        lines += ["", "Licence texts", "=============", ""]
        for text, names in sorted(texts.items(), key=lambda item: (item[1], item[0])):
            lines += ["-" * 72, "Used by: " + ", ".join(sorted(set(names))), "-" * 72, text, ""]
        (out / f"{program}.LICENSE.dependencies").write_text("\n".join(lines) + "\n")
    if args.summary:
        for package in sorted(set(PACKAGE_OF.values())):
            licences = [summaries[program] for program, owner in PACKAGE_OF.items() if owner == package]
            print(f"{package}: {expression(OWN + EXTRA.get(package, []) + licences)}")
    if args.check:
        tags = spec_licences()
        failed = False
        for package in sorted(set(PACKAGE_OF.values())):
            licences = [summaries[program] for program, owner in PACKAGE_OF.items() if owner == package]
            expected = expression(OWN + EXTRA.get(package, []) + licences)
            if tags.get(package) != expected:
                print(f"{package}: spec says {tags.get(package)!r}, crates say {expected!r}", file=sys.stderr)
                failed = True
        if failed:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
