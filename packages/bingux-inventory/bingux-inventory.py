#!/usr/bin/env python3
"""Collect installed package names and versions from local package managers."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Iterable, Sequence
from typing import Any

SCHEMA_VERSION = 2
COMMAND_TIMEOUT_SECONDS = 15.0
PackageRecord = dict[str, str]
Command = tuple[str, ...]

RPM_COMMAND: Command = (
    "rpm",
    "-qa",
    "--qf",
    "%{NAME}\t%{VERSION}-%{RELEASE}\n",
)
DNF_COMMANDS: tuple[Command, ...] = (
    (
        "dnf",
        "--cacheonly",
        "repoquery",
        "--installed",
        "--qf",
        "%{name}\t%{evr}\n",
    ),
    ("dnf", "--cacheonly", "list", "installed", "--quiet"),
)
FLATPAK_COMMAND: Command = (
    "flatpak",
    "list",
    "--app",
    "--columns=application,version",
)
CARGO_COMMAND: Command = ("cargo", "install", "--list")
PIPX_JSON_COMMAND: Command = ("pipx", "list", "--json")
PIPX_SHORT_COMMAND: Command = ("pipx", "list", "--short")
NPM_COMMAND: Command = ("npm", "ls", "--global", "--depth=0", "--json")
BUN_COMMANDS: tuple[Command, ...] = (
    ("bun", "pm", "ls", "--global"),
    ("bun", "pm", "ls", "-g"),
)
NIX_COMMAND: Command = ("nix", "profile", "list", "--json", "--offline")

CARGO_HEADER = re.compile(r"^\s*(?P<name>\S+)\s+v?(?P<version>\S+):\s*$")
BUN_PACKAGE = re.compile(
    r"^\s*(?:[├└]──\s+)?(?P<name>@[^@\s/]+/[^@\s]+|[^@\s]+)@(?P<version>\S+)\s*$"
)
NIX_VERSION = re.compile(r"-(?P<version>\d[0-9A-Za-z+._~:-]*)$")


def _safe_text(value: object) -> str | None:
    """Return a metadata value, excluding paths and control characters."""

    if not isinstance(value, str):
        return None
    text = value.strip()
    if not text or text.startswith(("/", "~")):
        return None
    if any(marker in text for marker in (":/", "=/", "://")):
        return None
    if any(character in text for character in ("\x00", "\r", "\n")):
        return None
    return text


def _record(name: object, version: object) -> PackageRecord | None:
    safe_name = _safe_text(name)
    safe_version = _safe_text(version)
    if safe_name is None or safe_version is None:
        return None
    return {"name": safe_name, "version": safe_version}


def _sorted_records(records: Iterable[PackageRecord]) -> list[PackageRecord]:
    unique = {(record["name"], record["version"]) for record in records}
    return [
        {"name": name, "version": version}
        for name, version in sorted(unique, key=lambda item: (item[0], item[1]))
    ]


def _run_command(command: Command) -> str | None:
    """Run one static, local listing command and return only its standard output."""

    if not command or shutil.which(command[0]) is None:
        return None

    try:
        result = subprocess.run(
            list(command),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
            shell=False,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=COMMAND_TIMEOUT_SECONDS,
        )
    except (OSError, subprocess.SubprocessError):
        return None

    if result.returncode != 0 and not result.stdout.strip():
        return None
    return result.stdout


def _parse_tabular(text: str) -> list[PackageRecord]:
    records: list[PackageRecord] = []
    for line in text.splitlines():
        fields = line.split("\t", 1)
        if len(fields) != 2:
            continue
        package = _record(fields[0], fields[1])
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _parse_dnf_list(text: str) -> list[PackageRecord]:
    records: list[PackageRecord] = []
    for line in text.splitlines():
        fields = line.split()
        if len(fields) < 2:
            continue
        package_name = fields[0]
        if package_name.lower() in {"installed", "available", "last"}:
            continue
        if package_name.startswith(("Last", "Error", "Warning")):
            continue
        package = _record(package_name, fields[1])
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _collect_rpm_or_dnf() -> tuple[str, list[PackageRecord]] | None:
    rpm_output = _run_command(RPM_COMMAND)
    if rpm_output is not None:
        return "rpm", _parse_tabular(rpm_output)

    for command in DNF_COMMANDS:
        dnf_output = _run_command(command)
        if dnf_output is None:
            continue
        if "\t" in dnf_output:
            return "dnf", _parse_tabular(dnf_output)
        return "dnf", _parse_dnf_list(dnf_output)
    return None


def _collect_flatpak() -> list[PackageRecord] | None:
    output = _run_command(FLATPAK_COMMAND)
    if output is None:
        return None
    return _parse_tabular(output)


def _collect_cargo() -> list[PackageRecord] | None:
    output = _run_command(CARGO_COMMAND)
    if output is None:
        return None

    records: list[PackageRecord] = []
    for line in output.splitlines():
        match = CARGO_HEADER.match(line)
        if match is None:
            continue
        package = _record(match.group("name"), match.group("version"))
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _parse_pipx_json(payload: object) -> list[PackageRecord] | None:
    if not isinstance(payload, dict):
        return None
    venvs = payload.get("venvs")
    if not isinstance(venvs, dict):
        return None

    records: list[PackageRecord] = []
    for venv_name, venv in venvs.items():
        if not isinstance(venv, dict):
            continue
        metadata = venv.get("metadata")
        main_package = metadata.get("main_package") if isinstance(metadata, dict) else None
        if not isinstance(main_package, dict):
            continue
        package_name = main_package.get("package") or venv_name
        package_version = main_package.get("version")
        package = _record(package_name, package_version)
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _parse_pipx_short(text: str) -> list[PackageRecord]:
    records: list[PackageRecord] = []
    for line in text.splitlines():
        fields = line.split()
        if len(fields) < 2:
            continue
        package = _record(fields[0], fields[1])
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _collect_pipx() -> list[PackageRecord] | None:
    json_output = _run_command(PIPX_JSON_COMMAND)
    if json_output is not None:
        try:
            payload: Any = json.loads(json_output)
        except json.JSONDecodeError:
            payload = None
        parsed = _parse_pipx_json(payload)
        if parsed is not None:
            return parsed

    short_output = _run_command(PIPX_SHORT_COMMAND)
    if short_output is None:
        return None
    return _parse_pipx_short(short_output)


def _parse_npm_json(payload: object) -> list[PackageRecord] | None:
    if not isinstance(payload, dict):
        return None
    dependencies = payload.get("dependencies")
    if dependencies is None:
        return []
    if not isinstance(dependencies, dict):
        return None

    records: list[PackageRecord] = []
    for package_name, metadata in dependencies.items():
        if not isinstance(metadata, dict):
            continue
        package = _record(package_name, metadata.get("version"))
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _collect_npm() -> list[PackageRecord] | None:
    output = _run_command(NPM_COMMAND)
    if output is None:
        return None
    try:
        payload: Any = json.loads(output)
    except json.JSONDecodeError:
        return None
    return _parse_npm_json(payload)


def _parse_bun(text: str) -> list[PackageRecord]:
    records: list[PackageRecord] = []
    for line in text.splitlines():
        match = BUN_PACKAGE.match(line)
        if match is None:
            continue
        package = _record(match.group("name"), match.group("version"))
        if package is not None:
            records.append(package)
    return _sorted_records(records)


def _collect_bun() -> list[PackageRecord] | None:
    for command in BUN_COMMANDS:
        output = _run_command(command)
        if output is not None:
            return _parse_bun(output)
    return None


def _nix_name(element_name: object, element: dict[str, object]) -> object:
    explicit_name = element.get("name")
    if explicit_name is not None:
        return explicit_name
    if element_name:
        return element_name
    attr_path = element.get("attrPath")
    if isinstance(attr_path, str):
        return attr_path.rsplit(".", 1)[-1]
    return None


def _nix_version(name: str, store_path: object) -> str | None:
    if not isinstance(store_path, str):
        return None
    basename = store_path.rsplit("/", 1)[-1]
    if not basename or basename == store_path:
        return None
    if "-" not in basename:
        return None
    store_name = basename.split("-", 1)[1]
    prefix = f"{name}-"
    if store_name.startswith(prefix):
        return _safe_text(store_name[len(prefix) :])
    match = NIX_VERSION.search(store_name)
    if match is None:
        return None
    return _safe_text(match.group("version"))


def _parse_nix_json(payload: object) -> list[PackageRecord] | None:
    if not isinstance(payload, dict):
        return None
    elements = payload.get("elements")
    if not isinstance(elements, dict):
        return None

    records: list[PackageRecord] = []
    for element_name, raw_element in elements.items():
        if not isinstance(raw_element, dict):
            continue
        if raw_element.get("active") is False:
            continue
        name = _safe_text(_nix_name(element_name, raw_element))
        if name is None:
            continue
        store_paths = raw_element.get("storePaths")
        if not isinstance(store_paths, list):
            continue
        for store_path in store_paths:
            version = _nix_version(name, store_path)
            package = _record(name, version)
            if package is not None:
                records.append(package)
    return _sorted_records(records)


def _collect_nix() -> list[PackageRecord] | None:
    output = _run_command(NIX_COMMAND)
    if output is None:
        return None
    try:
        payload: Any = json.loads(output)
    except json.JSONDecodeError:
        return None
    return _parse_nix_json(payload)


def _collect_sources() -> dict[str, list[PackageRecord]]:
    sources: dict[str, list[PackageRecord]] = {}

    rpm_or_dnf = _collect_rpm_or_dnf()
    if rpm_or_dnf is not None:
        source_name, records = rpm_or_dnf
        sources[source_name] = records

    collectors: tuple[tuple[str, Any], ...] = (
        ("flatpak", _collect_flatpak),
        ("cargo", _collect_cargo),
        ("pipx", _collect_pipx),
        ("npm", _collect_npm),
        ("bun", _collect_bun),
        ("nix", _collect_nix),
    )
    for source_name, collector in collectors:
        records = collector()
        if records is not None:
            sources[source_name] = records
    return sources


def _canonical_sources(
    sources: dict[str, Iterable[PackageRecord]],
) -> dict[str, list[PackageRecord]]:
    return {
        source_name: _sorted_records(records)
        for source_name, records in sources.items()
    }


def _inventory_json(pretty: bool) -> str:
    payload = {
        "schemaVersion": SCHEMA_VERSION,
        "sources": _canonical_sources(_collect_sources()),
    }
    if pretty:
        return json.dumps(payload, indent=2, sort_keys=True) + "\n"
    return json.dumps(payload, separators=(",", ":"), sort_keys=True) + "\n"


def _write_atomic(path_text: str, content: str) -> None:
    path = Path(path_text)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{path.name}.",
        dir=str(path.parent),
        text=True,
    )
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary_name, path)
    except BaseException:
        try:
            os.unlink(temporary_name)
        except OSError:
            pass
        raise


def _argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="bingux-inventory",
        description="Collect installed package metadata without network access.",
        allow_abbrev=False,
    )
    parser.add_argument(
        "--output",
        metavar="PATH",
        help="atomically write JSON to PATH instead of standard output",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="indent the JSON output for human readability",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _argument_parser().parse_args(argv)
    content = _inventory_json(arguments.pretty)
    if arguments.output is None:
        sys.stdout.write(content)
        return 0

    try:
        _write_atomic(arguments.output, content)
    except OSError:
        print("bingux-inventory: unable to write the requested output file", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
