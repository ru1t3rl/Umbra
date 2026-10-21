# Offline basemap assets (Natural Earth 1:50m)

Bundled for the offline map style (M5b WI-13, ADR 0041, human decision H-2 (a)). The app loads them from its own origin; there is no network access to a map provider by default.

## Source
- Natural Earth (https://www.naturalearthdata.com/), 1:50m physical and cultural vectors, **public domain** (https://www.naturalearthdata.com/about/terms-of-use/). No attribution is required.
- Taken from the `nvkelso/natural-earth-vector` repository, pinned to tag **v5.1.2** (commit `f1890d9f152c896d250a77557a5751a93d494776`), `geojson/` directory.

| Source file | URL | SHA-256 |
|---|---|---|
| `ne_50m_land.geojson` (1,636,166 bytes) | https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_land.geojson | `e874b27a51d146452be360cafb3cc50c86001074a67d534113e6534682f9826b` |
| `ne_50m_admin_0_boundary_lines_land.geojson` (760,189 bytes) | https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_admin_0_boundary_lines_land.geojson | `2faac4f6b34386f3d21b6e018cf151f241f00e5c936d44dd17d7d9bfb147fa48` |

## Files here (derived)
Douglas-Peucker tolerance 0.01 degrees (about 1.1 km), coordinates rounded to 3 decimals (about 110 m), all properties dropped, features sorted, compact JSON (ASM-500). Vertices on the antimeridian (|lon| = 180) or at latitude -89.9 or below are never dropped.

| File | Bytes | SHA-256 |
|---|---|---|
| `ne_50m_land.geojson` (polygons; 1418 of the 1422 source rings, the coast layer is drawn from this outline) | 926,272 | `30c00ec5318484bcd40536367c432f19b839332ade0c15f0ebc1bec8750e9ee4` |
| `ne_50m_admin0_lines.geojson` (land boundary lines) | 233,260 | `c9ce9aae8a82824d6b21139c197fc0f5310b7f30f653707a37ab8344fae00993` |

Total 1,159,532 bytes (budget: under 1.5 MB, enforced by `src/maplibre/basemapAssets.test.ts`, which also checks this table).

## Known limits
- There is no separate coastline file: the coast is the outline of the land polygons. That outline also draws the artificial cut lines at +-180 degrees (Chukotka, Fiji) and Antarctica's edges at -90 degrees, which are off-Mercator. Accepted cost of keeping fill and coast from drifting apart (ASM-500).
- `admin_0_boundary_lines_land` is Natural Earth's default de facto view of disputed borders. The lines are context only and imply no position on any border.

## Regenerating
Download the two source files from the URLs above into an empty directory, check the SHA-256 values, then, from `gui/`:

```sh
bun scripts/simplify-basemap.ts features/map-view/assets \
  <src>/ne_50m_land.geojson <src>/ne_50m_admin_0_boundary_lines_land.geojson
```

The script is deterministic: the output is byte-identical on every run (`gui/scripts/simplify-basemap.test.ts`). The source files are untrusted data and are only parsed as JSON. Changing the tolerance or precision means editing `TOLERANCE_DEG` / `PRECISION` in the script and updating the table above.
