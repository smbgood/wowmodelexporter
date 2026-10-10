# WoW 1.12 Blender Character Exporter

Windows-first pipeline for importing a manually configured World of Warcraft 1.12.1 character into Blender 4.x for posing and machinima prototyping.

## Project layout

- `converter/` - Rust CLI that validates a local WoW `Data` directory and exports a Blender-ready GLB.
- `blender_addon/wow_112_importer/` - Blender add-on that collects character settings, runs the converter, and imports the GLB.
- `wow_addon/WowModelExporterSnapshot/` - In-game WoW 1.12 addon that captures append-only gear snapshots.
- `tools/snapshot_ui.py` - Lightweight desktop UI to list snapshots and generate converter config JSON files.

## Current MVP scope

Implemented:

- Vanilla MPQ archive service with deterministic patch-first scan and byte caching.
- Bounds-checked parsers for M2 (MD20), optional SKIN sidecars, BLP1 (paletted + DXT1/DXT3), and DBC tables.
- All Vanilla race+sex profile routing (`human/orc/dwarf/nightelf/undead/tauren/gnome/troll`) plus explicit `highelf` alias.
- DBC-driven appearance lookup from `CharSections`, `CharHairGeosets`, `CharacterFacialHairStyles`, and `ItemDisplayInfo`.
- Character scene assembly with body + equipment primitives, attachment nodes, multi-material output, and bone hierarchy conversion.
- Texture decoding/compositing pipeline for body layers and wearable textures, with embedded PNG images in GLB.
- Generalized GLB export path for multi-mesh scenes, joints/weights, skins, and per-bone animation channels.
- Blender panel/operator wrapper that runs export and imports into the scene.
- WoW in-game one-click snapshot capture (`/wmesnap`) with append-only history in SavedVariables.

Deferred intentionally:

- Account/database lookup for characters from server backend.
- Full parity with every in-client material shader mode and render-state nuance.
- Snapshot-side appearance capture (skin/face/hair indices) from in-game API.
- Particles/ribbons/lights, enchant visuals, and sheathing state logic.

## Build the converter

Prerequisites:

- Rust stable toolchain (tested with `cargo 1.89+`).
- Blender 4.x.

Build:

```powershell
cd converter
cargo build --release
```

Binary:

- `converter/target/release/wowmodelexport-converter.exe`

## Converter configuration

Create a JSON config (example at `converter/examples/smbgood_config.json`):

```json
{
  "wow_data_path": "C:/Games/World of Warcraft/Data",
  "output_path": "C:/Temp/smbgood_character.glb",
  "profile": {
    "race": "nightelf",
    "sex": "female",
    "skin_index": 2,
    "face_index": 3,
    "hair_style_index": 4,
    "hair_color_index": 1,
    "facial_hair_index": 0
  },
  "equipment_display_ids": {
    "head": 12345,
    "shoulder": 12346,
    "chest": 12347,
    "hands": 12348,
    "mainhand": 12349
  },
  "animation_ids": [0, 4, 5, 69]
}
```

Commands:

```powershell
cd converter
cargo run -- validate --config .\examples\smbgood_config.json
cargo run -- export --config .\examples\smbgood_config.json
cargo run -- smoke-test --config .\examples\smbgood_config.json
```

## Blender add-on install and usage

1. In Blender: `Edit -> Preferences -> Add-ons -> Install...`
2. Select the `blender_addon/wow_112_importer` folder as a zip/package (or package it first).
3. Enable **WoW 1.12 Character Importer**.
4. In add-on preferences, set:
   - `Converter Executable` to `wowmodelexport-converter.exe`
   - `WoW Data Path` to your local 1.12 `Data` directory
5. Open `View3D -> Sidebar -> WoW 1.12`, fill character fields and display IDs, then click **Import WoW 1.12 Character**.
6. The imported rig is ready for posing; animation actions are available on the imported armature.

## Tests

Run synthetic tests only (no Blizzard assets checked in):

```powershell
cd converter
cargo test
```

The default integration smoke test creates a temporary fake `Data` directory and verifies that `validate` succeeds while `export` fails with a real archive-read error, protecting the runtime MPQ path.

Optional real-client integration coverage:

```powershell
$env:WME_WOW_DATA="C:\Games\World of Warcraft\Data"
cd converter
cargo test --test real_client_opt_in -- --nocapture
```

When `WME_WOW_DATA` is not set (or points to missing data), the real-client test auto-skips.

## WoW 1.12 in-game snapshot addon

Install:

1. Copy `wow_addon/WowModelExporterSnapshot` into your WoW `Interface/AddOns/` directory.
2. Launch the client and enable **WoW Model Exporter Snapshot**.

Usage:

- Click **Save Gear Snapshot** button (top-center) or run `/wmesnap`.
- Each press appends a new snapshot entry in `WME_SnapshotsDB.snapshots` (never overwrites prior entries).
- `/wmeexport` is an alias of `/wmesnap` and also prints a compact payload line for external lookup tooling.
- Compact payload history is also appended to `WME_SnapshotsDB.compactSnapshots` for easier downstream parsing.

Data captured per snapshot:

- Character metadata: realm, name, race, sex, class, level, timestamp.
- Equipment snapshot for slots 1-19: slot name, `itemLink`, parsed `itemId`, and icon texture.

Important persistence note:

- WoW addons cannot write arbitrary files directly during play; SavedVariables are persisted on `/reload` or logout.

## Snapshot import command

The converter can now parse addon SavedVariables snapshots and generate config JSON files:

```powershell
cd converter
cargo run -- import-snapshots `
  --saved-variables "B:\raven\RavenCraft\WTF\Account\<ACCOUNT>\<REALM>\<CHARACTER>\SavedVariables\WowModelExporterSnapshot.lua" `
  --output-dir ".\generated_snapshot_configs" `
  --wow-data-path "B:\raven\RavenCraft\Data" `
  --output-glb-dir "B:\raven\RavenCraft\exports" `
  --list
```

Generate one config for snapshot id 3:

```powershell
cargo run -- import-snapshots `
  --saved-variables "B:\raven\RavenCraft\WTF\Account\<ACCOUNT>\<REALM>\<CHARACTER>\SavedVariables\WowModelExporterSnapshot.lua" `
  --output-dir ".\generated_snapshot_configs" `
  --wow-data-path "B:\raven\RavenCraft\Data" `
  --output-glb-dir "B:\raven\RavenCraft\exports" `
  --snapshot-id 3
```

Generate with automatic `itemId -> displayId` resolution:

```powershell
cargo run -- import-snapshots `
  --saved-variables "B:\raven\RavenCraft\WTF\Account\<ACCOUNT>\<REALM>\<CHARACTER>\SavedVariables\WowModelExporterSnapshot.lua" `
  --output-dir ".\generated_snapshot_configs" `
  --wow-data-path "B:\raven\RavenCraft\Data" `
  --output-glb-dir "B:\raven\RavenCraft\exports" `
  --item-display-lookup ".\examples\item_display_lookup_sample.json"
```

Generate using direct DB export file (SQL/CSV):

```powershell
cargo run -- import-snapshots `
  --saved-variables "B:\raven\RavenCraft\WTF\Account\<ACCOUNT>\<REALM>\<CHARACTER>\SavedVariables\WowModelExporterSnapshot.lua" `
  --output-dir ".\generated_snapshot_configs" `
  --wow-data-path "B:\raven\RavenCraft\Data" `
  --output-glb-dir "B:\raven\RavenCraft\exports" `
  --db-export-lookup ".\examples\item_template_export_sample.sql"
```

Notes:

- Generated JSON includes `snapshot_item_ids` from in-game item IDs.
- With `--item-display-lookup`, `equipment_display_ids` is automatically populated from your lookup table.
- With `--db-export-lookup`, mappings are parsed directly from DB export rows (`entry/itemId` + `displayId`).
- If both are provided, `--item-display-lookup` overrides conflicting rows from `--db-export-lookup`.
- Any unmapped item IDs are written to `unresolved_item_ids` in each generated config.
- Optional flag `--use-item-ids-as-display-ids` copies item IDs into `equipment_display_ids` as a fallback.

## Lightweight snapshot UI

Run:

```powershell
python .\tools\snapshot_ui.py
```

Workflow:

1. Set converter executable and SavedVariables Lua path.
2. Optionally set DB export file (SQL/CSV) and/or item->display lookup JSON/CSV.
3. Click **Refresh Snapshots** for a glanceable table.
4. Select a row and click **Generate Selected Config** (or generate all).
5. Click **Generate Model from Config...** and select a generated config to export its GLB.

## Asset/legal safeguards

- Do not commit Blizzard client assets, extracted model files, textures, DBC data, or generated character outputs.
- This repository only stores code and synthetic tests.
- Users must provide their own local WoW 1.12 client installation and keep exported content local.
