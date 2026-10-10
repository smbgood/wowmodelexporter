use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotRecord {
    pub snapshot_id: usize,
    pub name: Option<String>,
    pub realm: Option<String>,
    pub race: Option<String>,
    pub sex: Option<String>,
    pub class_name: Option<String>,
    pub gear_item_ids: BTreeMap<String, u32>,
    pub raw_payload: String,
}

#[derive(Debug, Clone)]
pub struct ImportSnapshotsOptions {
    pub saved_variables_path: PathBuf,
    pub output_dir: PathBuf,
    pub wow_data_path: PathBuf,
    pub output_glb_dir: PathBuf,
    pub snapshot_id: Option<usize>,
    pub use_item_ids_as_display_ids: bool,
    pub item_display_lookup_path: Option<PathBuf>,
    pub db_export_lookup_path: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ImportSnapshotsResult {
    pub found: usize,
    pub generated: Vec<PathBuf>,
    pub snapshots: Vec<SnapshotRecord>,
    pub resolved_slots: usize,
    pub unresolved_slots: usize,
}

pub fn list_snapshots(saved_variables_path: &Path) -> Result<Vec<SnapshotRecord>> {
    let raw = fs::read_to_string(saved_variables_path)
        .with_context(|| format!("failed to read {}", saved_variables_path.display()))?;
    parse_snapshots_from_savedvariables(&raw)
}

pub fn import_snapshots(options: &ImportSnapshotsOptions) -> Result<ImportSnapshotsResult> {
    let snapshots = list_snapshots(&options.saved_variables_path)?;
    if snapshots.is_empty() {
        bail!("no WME snapshots found in {}", options.saved_variables_path.display());
    }
    fs::create_dir_all(&options.output_dir)
        .with_context(|| format!("failed to create {}", options.output_dir.display()))?;

    let selected: Vec<&SnapshotRecord> = match options.snapshot_id {
        Some(id) => snapshots.iter().filter(|snap| snap.snapshot_id == id).collect(),
        None => snapshots.iter().collect(),
    };
    if selected.is_empty() {
        bail!(
            "snapshot id {} not found (found {} total)",
            options.snapshot_id.unwrap_or_default(),
            snapshots.len()
        );
    }

    let resolver = load_merged_lookup(
        options.db_export_lookup_path.as_deref(),
        options.item_display_lookup_path.as_deref(),
    )?;

    let mut generated = Vec::new();
    let mut resolved_slots = 0usize;
    let mut unresolved_slots = 0usize;
    for snap in selected {
        let path = options
            .output_dir
            .join(format!("snapshot_{:04}.json", snap.snapshot_id));
        let resolved = resolve_snapshot_display_ids(snap, &resolver, options.use_item_ids_as_display_ids);
        resolved_slots += resolved.resolved.len();
        unresolved_slots += resolved.unresolved.len();
        let json = build_config_json(snap, options, resolved);
        let bytes = serde_json::to_vec_pretty(&json).context("serialize snapshot config JSON")?;
        fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
        generated.push(path);
    }

    Ok(ImportSnapshotsResult {
        found: snapshots.len(),
        generated,
        snapshots,
        resolved_slots,
        unresolved_slots,
    })
}

pub fn parse_snapshots_from_savedvariables(raw: &str) -> Result<Vec<SnapshotRecord>> {
    let payload_re = Regex::new(r#"WME_SNAPSHOT\|[^"\r\n]+"#).expect("valid regex");
    let mut snapshots = Vec::new();
    let mut seen = BTreeSet::new();
    for mat in payload_re.find_iter(raw) {
        let payload = mat.as_str().to_string();
        if !seen.insert(payload.clone()) {
            continue;
        }
        let next_id = snapshots.len() + 1;
        snapshots.push(parse_payload(next_id, &payload)?);
    }
    Ok(snapshots)
}

#[derive(Debug, Default, Clone)]
struct DisplayIdResolution {
    resolved: BTreeMap<String, u32>,
    unresolved: BTreeMap<String, u32>,
}

fn parse_payload(snapshot_id: usize, payload: &str) -> Result<SnapshotRecord> {
    if !payload.starts_with("WME_SNAPSHOT|") {
        bail!("invalid payload prefix");
    }

    let mut map = BTreeMap::<String, String>::new();
    for chunk in payload.split('|').skip(1) {
        if let Some((k, v)) = chunk.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let mut gear_item_ids = BTreeMap::new();
    for (k, v) in &map {
        if matches!(
            k.as_str(),
            "head"
                | "neck"
                | "shoulder"
                | "shirt"
                | "chest"
                | "waist"
                | "legs"
                | "feet"
                | "wrist"
                | "hands"
                | "finger1"
                | "finger2"
                | "trinket1"
                | "trinket2"
                | "back"
                | "mainhand"
                | "offhand"
                | "ranged"
                | "tabard"
        ) && let Ok(id) = v.parse::<u32>()
        {
            gear_item_ids.insert(k.clone(), id);
        }
    }

    Ok(SnapshotRecord {
        snapshot_id,
        name: map.get("name").cloned(),
        realm: map.get("realm").cloned(),
        race: map.get("race").cloned(),
        sex: map.get("sex").cloned(),
        class_name: map.get("class").cloned(),
        gear_item_ids,
        raw_payload: payload.to_string(),
    })
}

fn build_config_json(
    snap: &SnapshotRecord,
    options: &ImportSnapshotsOptions,
    resolution: DisplayIdResolution,
) -> Value {
    let race = normalize_race(snap.race.as_deref().unwrap_or("nightelf"));
    let sex = normalize_sex(snap.sex.as_deref().unwrap_or("female"));
    let output_glb = options
        .output_glb_dir
        .join(format!("snapshot_{:04}_{}.glb", snap.snapshot_id, race));

    json!({
        "wow_data_path": options.wow_data_path,
        "output_path": output_glb,
        "profile": {
            "race": race,
            "sex": sex,
            "skin_index": 0,
            "face_index": 0,
            "hair_style_index": 0,
            "hair_color_index": 0,
            "facial_hair_index": 0
        },
        "equipment_display_ids": resolution.resolved,
        "unresolved_item_ids": resolution.unresolved,
        "snapshot_item_ids": snap.gear_item_ids,
        "snapshot_meta": {
            "snapshot_id": snap.snapshot_id,
            "name": snap.name,
            "realm": snap.realm,
            "race": snap.race,
            "sex": snap.sex,
            "class": snap.class_name,
            "raw_payload": snap.raw_payload,
            "note": if options.item_display_lookup_path.is_some() {
                "equipment_display_ids resolved using DB export + lookup file where available; unresolved_item_ids require manual/DB resolution"
            } else if options.db_export_lookup_path.is_some() {
                "equipment_display_ids resolved using DB export file where available; unresolved_item_ids require manual resolution"
            } else if options.use_item_ids_as_display_ids {
                "lookup file not provided; item IDs were copied into equipment_display_ids as fallback"
            } else {
                "lookup file not provided; equipment_display_ids includes only resolvable entries (likely empty)"
            }
        },
        "animation_ids": [0, 4, 5, 69]
    })
}

fn resolve_snapshot_display_ids(
    snap: &SnapshotRecord,
    lookup: &BTreeMap<u32, u32>,
    fallback_to_item_ids: bool,
) -> DisplayIdResolution {
    let mut out = DisplayIdResolution::default();
    for (slot, item_id) in &snap.gear_item_ids {
        if let Some(display_id) = lookup.get(item_id) {
            out.resolved.insert(slot.clone(), *display_id);
        } else if fallback_to_item_ids {
            out.resolved.insert(slot.clone(), *item_id);
        } else {
            out.unresolved.insert(slot.clone(), *item_id);
        }
    }
    out
}

fn load_item_display_lookup(path: Option<&Path>) -> Result<BTreeMap<u32, u32>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = parse_lookup_json(&raw).or_else(|_| parse_lookup_delimited(&raw))?;
    if parsed.is_empty() {
        bail!("lookup file {} did not contain any mappings", path.display());
    }
    Ok(parsed)
}

fn load_db_export_lookup(path: Option<&Path>) -> Result<BTreeMap<u32, u32>> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let parsed = parse_db_export_sql(&raw)
        .or_else(|_| parse_db_export_delimited(&raw))
        .or_else(|_| parse_lookup_json(&raw))
        .or_else(|_| parse_lookup_delimited(&raw))?;
    if parsed.is_empty() {
        bail!(
            "DB export lookup file {} did not contain any mappings",
            path.display()
        );
    }
    Ok(parsed)
}

fn load_merged_lookup(
    db_export_path: Option<&Path>,
    explicit_lookup_path: Option<&Path>,
) -> Result<BTreeMap<u32, u32>> {
    let mut merged = load_db_export_lookup(db_export_path)?;
    let explicit = load_item_display_lookup(explicit_lookup_path)?;
    // Explicit lookup file takes precedence so users can override DB data.
    for (item_id, display_id) in explicit {
        merged.insert(item_id, display_id);
    }
    Ok(merged)
}

fn parse_db_export_sql(raw: &str) -> Result<BTreeMap<u32, u32>> {
    let insert_re = Regex::new(
        r#"(?is)insert\s+into\s+`?(?P<table>\w+)`?\s*(\((?P<cols>[^)]*)\))?\s*values\s*(?P<values>.*?);"#,
    )
    .expect("valid regex");
    let mut out = BTreeMap::new();

    for cap in insert_re.captures_iter(raw) {
        let table = cap
            .name("table")
            .map(|m| m.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        if !(table.contains("item") || table.contains("template")) {
            continue;
        }

        let cols = cap.name("cols").map(|m| parse_sql_columns(m.as_str()));
        let values_blob = cap
            .name("values")
            .map(|m| m.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing values block"))?;
        let tuples = split_sql_tuples(values_blob);

        let (item_col_idx, display_col_idx) = if let Some(columns) = cols {
            (
                find_item_column_index(&columns)
                    .ok_or_else(|| anyhow::anyhow!("missing item id column in SQL export"))?,
                find_display_column_index(&columns)
                    .ok_or_else(|| anyhow::anyhow!("missing display id column in SQL export"))?,
            )
        } else {
            // Fallback for value-only inserts from common 1.12 schemas.
            (0usize, 5usize)
        };

        for tuple in tuples {
            let fields = split_sql_fields(&tuple);
            if item_col_idx >= fields.len() || display_col_idx >= fields.len() {
                continue;
            }
            let Some(item_id) = parse_u32_field(&fields[item_col_idx]) else {
                continue;
            };
            let Some(display_id) = parse_u32_field(&fields[display_col_idx]) else {
                continue;
            };
            out.insert(item_id, display_id);
        }
    }

    if out.is_empty() {
        bail!("no item->display mappings found in SQL export");
    }
    Ok(out)
}

fn parse_db_export_delimited(raw: &str) -> Result<BTreeMap<u32, u32>> {
    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.is_empty() {
        bail!("empty DB export");
    }

    let header = lines[0];
    let delim = if header.contains('\t') {
        '\t'
    } else if header.contains(';') {
        ';'
    } else {
        ','
    };
    let columns: Vec<String> = header
        .split(delim)
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    let Some(item_idx) = find_item_column_index(&columns) else {
        bail!("DB export missing item id header (entry/itemid)");
    };
    let Some(display_idx) = find_display_column_index(&columns) else {
        bail!("DB export missing display id header");
    };

    let mut out = BTreeMap::new();
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split(delim).map(str::trim).collect();
        if item_idx >= parts.len() || display_idx >= parts.len() {
            continue;
        }
        let Some(item_id) = parse_u32_field(parts[item_idx]) else {
            continue;
        };
        let Some(display_id) = parse_u32_field(parts[display_idx]) else {
            continue;
        };
        out.insert(item_id, display_id);
    }
    if out.is_empty() {
        bail!("no rows parsed from DB export delimited file");
    }
    Ok(out)
}

fn parse_lookup_json(raw: &str) -> Result<BTreeMap<u32, u32>> {
    let value: Value = serde_json::from_str(raw).context("invalid lookup JSON")?;
    let mut map = BTreeMap::new();
    match value {
        Value::Object(obj) => {
            for (k, v) in obj {
                let item_id = k
                    .parse::<u32>()
                    .with_context(|| format!("invalid item id key '{}'", k))?;
                let display_id = v
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("invalid display id value for item {}", item_id))?
                    as u32;
                map.insert(item_id, display_id);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                let item_id = row
                    .get("itemId")
                    .or_else(|| row.get("item_id"))
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("lookup JSON array row missing itemId"))?
                    as u32;
                let display_id = row
                    .get("displayId")
                    .or_else(|| row.get("display_id"))
                    .and_then(Value::as_u64)
                    .ok_or_else(|| anyhow::anyhow!("lookup JSON array row missing displayId"))?
                    as u32;
                map.insert(item_id, display_id);
            }
        }
        _ => bail!("lookup JSON must be object or array"),
    }
    Ok(map)
}

fn parse_lookup_delimited(raw: &str) -> Result<BTreeMap<u32, u32>> {
    let mut out = BTreeMap::new();
    let mut found = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let delim = if line.contains(',') {
            ','
        } else if line.contains('\t') {
            '\t'
        } else if line.contains(';') {
            ';'
        } else {
            continue;
        };
        let cols: Vec<&str> = line.split(delim).map(str::trim).collect();
        if cols.len() < 2 {
            continue;
        }

        let left = cols[0];
        let right = cols[1];
        if left.eq_ignore_ascii_case("itemid")
            || left.eq_ignore_ascii_case("item_id")
            || !left.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        if !right.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let item_id = left.parse::<u32>()?;
        let display_id = right.parse::<u32>()?;
        out.insert(item_id, display_id);
        found = true;
    }
    if !found {
        bail!("no delimited item->display rows found");
    }
    Ok(out)
}

fn parse_sql_columns(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|col| {
            col.trim()
                .trim_matches('`')
                .trim_matches('"')
                .to_ascii_lowercase()
        })
        .collect()
}

fn split_sql_tuples(values_blob: &str) -> Vec<String> {
    let mut tuples = Vec::new();
    let mut depth = 0usize;
    let mut in_quote = false;
    let mut start: Option<usize> = None;
    let chars: Vec<char> = values_blob.chars().collect();

    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            let escaped = i > 0 && chars[i - 1] == '\\';
            if !escaped {
                in_quote = !in_quote;
            }
        } else if !in_quote {
            if c == '(' {
                if depth == 0 {
                    start = Some(i + 1);
                }
                depth += 1;
            } else if c == ')' && depth > 0 {
                depth -= 1;
                if depth == 0 && let Some(s) = start.take() {
                    tuples.push(chars[s..i].iter().collect());
                }
            }
        }
        i += 1;
    }
    tuples
}

fn split_sql_fields(tuple: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quote = false;
    let chars: Vec<char> = tuple.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            let escaped = i > 0 && chars[i - 1] == '\\';
            if !escaped {
                in_quote = !in_quote;
            }
            buf.push(c);
        } else if c == ',' && !in_quote {
            out.push(buf.trim().to_string());
            buf.clear();
        } else {
            buf.push(c);
        }
        i += 1;
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn parse_u32_field(token: &str) -> Option<u32> {
    let token = token.trim().trim_matches('\'').trim_matches('"');
    if token.eq_ignore_ascii_case("null") || token.is_empty() {
        return None;
    }
    token.parse::<u32>().ok()
}

fn find_item_column_index(columns: &[String]) -> Option<usize> {
    columns.iter().position(|c| {
        c == "entry" || c == "item" || c == "itemid" || c == "item_id" || c == "id"
    })
}

fn find_display_column_index(columns: &[String]) -> Option<usize> {
    columns.iter().position(|c| {
        c == "displayid"
            || c == "display_id"
            || c == "displayid1"
            || c == "displayid_1"
            || c == "displayinfoid"
    })
}

fn normalize_race(raw: &str) -> String {
    let norm = raw.to_ascii_lowercase().replace([' ', '_', '-'], "");
    match norm.as_str() {
        "nightelf" => "nightelf".to_string(),
        "highelf" => "highelf".to_string(),
        "scourge" => "undead".to_string(),
        "" => "nightelf".to_string(),
        _ => norm,
    }
}

fn normalize_sex(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "male" | "m" => "male".to_string(),
        _ => "female".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_payloads_from_savedvars() {
        let raw = r#"
WME_SnapshotsDB = {
  ["lastPayload"] = "WME_SNAPSHOT|class=Warrior|head=11726|name=smbgood|race=NightElf|realm=RavenCraft|sex=female",
  ["compactSnapshots"] = {
    "WME_SNAPSHOT|class=Warrior|head=11726|name=smbgood|race=NightElf|realm=RavenCraft|sex=female",
    "WME_SNAPSHOT|class=Warrior|mainhand=19334|name=smbgood|race=NightElf|realm=RavenCraft|sex=female",
  }
}
"#;
        let snaps = parse_snapshots_from_savedvariables(raw).expect("snapshots");
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].name.as_deref(), Some("smbgood"));
        assert_eq!(snaps[0].gear_item_ids.get("head"), Some(&11726u32));
        assert_eq!(snaps[1].gear_item_ids.get("mainhand"), Some(&19334u32));
    }

    #[test]
    fn parses_lookup_json_object() {
        let map = parse_lookup_json(r#"{"11726":3456,"19334":7890}"#).expect("lookup");
        assert_eq!(map.get(&11726), Some(&3456));
    }

    #[test]
    fn resolves_display_ids_with_fallback() {
        let snap = SnapshotRecord {
            snapshot_id: 1,
            name: None,
            realm: None,
            race: Some("NightElf".to_string()),
            sex: Some("female".to_string()),
            class_name: None,
            gear_item_ids: BTreeMap::from([
                ("head".to_string(), 11726),
                ("mainhand".to_string(), 99999),
            ]),
            raw_payload: "WME_SNAPSHOT|head=11726|mainhand=99999".to_string(),
        };
        let lookup = BTreeMap::from([(11726u32, 3456u32)]);
        let resolved = resolve_snapshot_display_ids(&snap, &lookup, true);
        assert_eq!(resolved.resolved.get("head"), Some(&3456u32));
        assert_eq!(resolved.resolved.get("mainhand"), Some(&99999u32));
        assert!(resolved.unresolved.is_empty());
    }

    #[test]
    fn parses_db_export_delimited_headers() {
        let raw = "entry,displayid,name\n11726,20115,foo\n19334,31959,bar\n";
        let map = parse_db_export_delimited(raw).expect("db csv");
        assert_eq!(map.get(&11726), Some(&20115));
        assert_eq!(map.get(&19334), Some(&31959));
    }

    #[test]
    fn parses_db_export_sql_insert() {
        let raw = "INSERT INTO `item_template` (`entry`,`class`,`subclass`,`name`,`displayid`) VALUES (11726,2,0,'Lifegiver',20115),(19334,2,0,'The Untamed Blade',31959);";
        let map = parse_db_export_sql(raw).expect("db sql");
        assert_eq!(map.get(&11726), Some(&20115));
        assert_eq!(map.get(&19334), Some(&31959));
    }
}
