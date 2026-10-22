# Installer assets

| File | Source | Licence |
|---|---|---|
| `world-map.svg` | Natural Earth 110m land (`ne_110m_land.geojson`), converted by `make-assets.py` | Public domain (Natural Earth terms of use) |
| `timezones.json` | `zone1970.tab` from the IANA tz database, converted by `make-assets.py` | Public domain (IANA tz database) |
| `avatar-1.svg` to `avatar-6.svg` | Drawn for Bingux; not derived from third-party art | Same licence as the Bingux repository |

Regenerate the map and zone data with:

    make-assets.py --land ne_110m_land.geojson --zone1970 /usr/share/zoneinfo/zone1970.tab
