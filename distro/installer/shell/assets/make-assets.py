#!/usr/bin/env python3
"""Regenerate the installer's map and timezone assets from public-domain sources.

    make-assets.py --land ne_110m_land.geojson --zone1970 /usr/share/zoneinfo/zone1970.tab

Writes world-map.svg (equirectangular land, lat 85 to -60) and timezones.json (one entry per
zone in zone1970.tab with its lat/lon). See SOURCES.md for licences. Output is deterministic.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re

HERE = pathlib.Path(__file__).resolve().parent
LAT_TOP = 85.0
LAT_BOTTOM = -60.0


def ring_path(ring: list[list[float]]) -> str:
    parts = []
    for index, (lon, lat) in enumerate(ring):
        command = "M" if index == 0 else "L"
        parts.append(f"{command}{lon:.2f} {-lat:.2f}")
    return "".join(parts) + "Z"


def world_svg(land_path: pathlib.Path) -> str:
    data = json.loads(land_path.read_text(encoding="utf-8"))
    paths: list[str] = []
    for feature in data["features"]:
        geometry = feature["geometry"]
        polygons = geometry["coordinates"] if geometry["type"] == "MultiPolygon" else [geometry["coordinates"]]
        for polygon in polygons:
            for ring in polygon:
                kept = [point for point in ring if LAT_BOTTOM <= point[1] <= LAT_TOP]
                if len(kept) >= 3:
                    paths.append(ring_path(kept))
    height = LAT_TOP - LAT_BOTTOM
    body = "".join(f'<path d="{path}"/>' for path in paths)
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="-180 %g 360 %g">\n'
        '<title>World land, equirectangular, public domain (Natural Earth 110m)</title>\n'
        '<g fill="#8a939e" stroke="none">%s</g>\n'
        "</svg>\n" % (-LAT_TOP, height, body)
    )


def dms(text: str) -> float:
    """Parse ISO 6709 degrees and optional minutes/seconds, e.g. +4230 or +503315."""
    sign = -1.0 if text[0] == "-" else 1.0
    digits = text[1:]
    if len(digits) in (4, 6):
        degrees = int(digits[:2])
        minutes = int(digits[2:4])
        seconds = int(digits[4:6]) if len(digits) == 6 else 0
    else:
        degrees = int(digits[:3])
        minutes = int(digits[3:5])
        seconds = int(digits[5:7]) if len(digits) == 7 else 0
    return sign * (degrees + minutes / 60 + seconds / 3600)


def coordinates(text: str) -> tuple[float, float]:
    match = re.fullmatch(r"([+-]\d{4,6})([+-]\d{5,7})", text)
    if not match:
        raise ValueError(f"bad coordinates: {text}")
    return dms(match.group(1)), dms(match.group(2))


def timezones_json(zone_tab: pathlib.Path) -> list[dict[str, object]]:
    zones = []
    for line in zone_tab.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        columns = line.split("\t")
        lat, lon = coordinates(columns[1])
        zone = columns[2]
        zones.append({"zone": zone, "lat": round(lat, 4), "lon": round(lon, 4)})
    zones.sort(key=lambda item: item["zone"])
    return zones


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--land", required=True, type=pathlib.Path)
    parser.add_argument("--zone1970", required=True, type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path, default=HERE)
    args = parser.parse_args()
    (args.out / "world-map.svg").write_text(world_svg(args.land), encoding="utf-8")
    (args.out / "timezones.json").write_text(
        json.dumps(timezones_json(args.zone1970), indent=1, sort_keys=True) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
