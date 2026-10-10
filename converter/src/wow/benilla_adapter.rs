use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{ExportConfig, Sex};
use crate::wow::archive::ArchiveSet;
use crate::wow::blp::decode_blp_rgba;
use crate::wow::dbc::{
    CharSectionRecord, CharacterFacialHairStyleRecord, ItemDisplayInfoRecord,
    decode_char_hair_geosets, decode_char_sections, decode_character_facial_hair_styles,
    decode_item_display_info, normalize_model_path, normalize_texture_path, parse_dbc_table,
};
use crate::wow::m2::parse_m2;
use crate::wow::skin::parse_skin;
use crate::wow::types::{
    AppearanceRecipe, CharacterSourceData, ClientBuildInfo, EquipmentRecipe, ModelAsset,
};
use crate::wow::{WowDataAdapter, WowDataSummary};

pub const BENILLA_PINNED_COMMIT: &str = "samwhosung/benilla@d1a6f67";
const SUPPORTED_BUILD: u16 = 5875;

pub struct BenillaAdapter {
    data_path: PathBuf,
}

#[derive(Debug, Clone)]
struct ResolvedProfile {
    normalized_race: String,
    canonical_race: String,
    race_id: i32,
    gender_id: i32,
    sex: Sex,
    base_model_path: String,
    notes: Vec<String>,
}

impl BenillaAdapter {
    pub fn new(data_path: &Path) -> Self {
        Self {
            data_path: data_path.to_path_buf(),
        }
    }

    fn open_archives(&self) -> Result<ArchiveSet> {
        ArchiveSet::open(&self.data_path)
    }

    fn read_file_from_archives(&self, file_path: &str) -> Result<Vec<u8>> {
        self.open_archives()?.read_file(file_path)
    }

    fn read_dbc_table(&self, dbc_name: &str) -> Result<crate::wow::dbc::DbcTable> {
        let path = format!("DBFilesClient\\{dbc_name}.dbc");
        let bytes = self.read_file_from_archives(&path)?;
        parse_dbc_table(&bytes).with_context(|| format!("parse {}", path))
    }

    fn read_model_asset(&self, m2_path: &str) -> Result<ModelAsset> {
        let bytes = self.read_file_from_archives(m2_path)?;
        let model = parse_m2(&bytes).with_context(|| format!("parse {}", m2_path))?;
        let mut skin = None;
        for i in 0..4 {
            let skin_path = if let Some(base) = m2_path.strip_suffix(".m2") {
                format!("{base}{i:02}.skin")
            } else {
                format!("{m2_path}{i:02}.skin")
            };
            if let Ok(bytes) = self.read_file_from_archives(&skin_path)
                && let Ok(parsed) = parse_skin(&bytes)
            {
                skin = Some(parsed);
                break;
            }
        }
        Ok(ModelAsset {
            model_path: normalize_model_path(m2_path),
            model,
            skin,
        })
    }

    fn resolve_profile(&self, cfg: &ExportConfig) -> Result<ResolvedProfile> {
        let normalized = cfg
            .profile
            .race
            .to_lowercase()
            .replace([' ', '_', '-'], "");
        let sex = cfg.profile.sex.clone();
        let gender_id = match sex {
            Sex::Male => 0,
            Sex::Female => 1,
        };
        let sex_label = match sex {
            Sex::Male => "Male",
            Sex::Female => "Female",
        };

        let (canonical_race, race_id, race_folder, race_model, alias_note) = match normalized.as_str()
        {
            "human" => ("human", 1, "Human", "Human", None),
            "orc" => ("orc", 2, "Orc", "Orc", None),
            "dwarf" => ("dwarf", 3, "Dwarf", "Dwarf", None),
            "nightelf" => ("nightelf", 4, "NightElf", "NightElf", None),
            "undead" => ("undead", 5, "Scourge", "Scourge", None),
            "tauren" => ("tauren", 6, "Tauren", "Tauren", None),
            "gnome" => ("gnome", 7, "Gnome", "Gnome", None),
            "troll" => ("troll", 8, "Troll", "Troll", None),
            "highelf" => (
                "nightelf",
                4,
                "NightElf",
                "NightElf",
                Some("high elf alias currently mapped to night elf assets"),
            ),
            _ => {
                bail!(
                    "unsupported race '{}'; expected one of human/orc/dwarf/nightelf/undead/tauren/gnome/troll or highelf alias",
                    cfg.profile.race
                )
            }
        };

        let base_model_path =
            format!("Character/{race_folder}/{sex_label}/{race_model}{sex_label}.m2");
        let mut notes = vec![
            format!("vanilla profile route active for race={canonical_race} sex={sex_label}"),
            "character customization assembled from CharSections/Hair/FacialHair tables"
                .to_string(),
        ];
        if let Some(note) = alias_note {
            notes.push(note.to_string());
        }
        Ok(ResolvedProfile {
            normalized_race: normalized,
            canonical_race: canonical_race.to_string(),
            race_id,
            gender_id,
            sex,
            base_model_path,
            notes,
        })
    }

    fn attachment_for_slot(slot: &str) -> &str {
        match slot {
            "head" => "helmet",
            "shoulder" => "shoulder",
            "chest" => "chest",
            "hands" => "hands",
            "mainhand" => "right_hand",
            "offhand" => "left_hand",
            _ => "generic",
        }
    }

    fn pick_section<'a>(
        sections: &'a [CharSectionRecord],
        race_id: i32,
        gender_id: i32,
        section_type: i32,
        preferred_color: i32,
        preferred_variation: i32,
    ) -> Option<&'a CharSectionRecord> {
        let mut candidates: Vec<&CharSectionRecord> = sections
            .iter()
            .filter(|row| {
                row.race == race_id && row.gender == gender_id && row.section_type == section_type
            })
            .collect();
        candidates.sort_by_key(|row| (row.color, row.variation, row.id));

        candidates
            .iter()
            .copied()
            .find(|row| row.color == preferred_color && row.variation == preferred_variation)
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|row| row.variation == preferred_variation)
            })
            .or_else(|| {
                candidates
                    .iter()
                    .copied()
                    .find(|row| row.color == preferred_color)
            })
            .or_else(|| candidates.first().copied())
    }

    fn collect_section_textures(section: Option<&CharSectionRecord>) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(section) = section {
            for texture in [&section.texture_1, &section.texture_2, &section.texture_3] {
                if let Some(path) = texture
                    .as_ref()
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| normalize_texture_path(v))
                {
                    out.push(path);
                }
            }
        }
        out
    }

    fn pick_item_asset(display: &ItemDisplayInfoRecord, offhand: bool) -> (Option<String>, Option<String>) {
        let model_primary = if offhand {
            display.model_name_2.as_deref().or(display.model_name_1.as_deref())
        } else {
            display.model_name_1.as_deref().or(display.model_name_2.as_deref())
        };

        let texture_primary = if offhand {
            display.texture_2.as_deref().or(display.texture_1.as_deref())
        } else {
            display.texture_1.as_deref().or(display.texture_2.as_deref())
        };

        let model = model_primary
            .filter(|v| !v.trim().is_empty())
            .map(normalize_model_path);
        let texture = texture_primary
            .filter(|v| !v.trim().is_empty())
            .map(normalize_texture_path);
        (model, texture)
    }

    fn read_blp_rgba(&self, texture_path: &str) -> Option<(u32, u32, Vec<u8>)> {
        let bytes = self.read_file_from_archives(texture_path).ok()?;
        let img = decode_blp_rgba(&bytes).ok()?;
        Some((img.width, img.height, img.rgba))
    }
}

fn resolve_hair_geoset(
    rows: &[crate::wow::dbc::CharHairGeosetRecord],
    race_id: i32,
    gender_id: i32,
    variation: i32,
) -> u16 {
    rows.iter()
        .find(|r| r.race == race_id && r.gender == gender_id && r.variation == variation)
        .map(|r| r.geoset.max(0) as u16)
        .unwrap_or(700 + (variation.max(0) as u16 % 32))
}

fn resolve_facial_geoset(
    rows: &[CharacterFacialHairStyleRecord],
    race_id: i32,
    gender_id: i32,
    variation: i32,
) -> u16 {
    rows.iter()
        .find(|r| r.race == race_id && r.gender == gender_id && r.variation == variation)
        .and_then(|r| r.geosets.iter().copied().find(|g| *g > 0))
        .map(|g| g as u16)
        .unwrap_or(900 + (variation.max(0) as u16 % 32))
}

fn composite_layers(layers: &[(u32, u32, Vec<u8>)]) -> Option<(u32, u32, Vec<u8>)> {
    let first = layers.first()?;
    let width = first.0;
    let height = first.1;
    let mut out = first.2.clone();
    for (_, _, layer) in layers.iter().skip(1) {
        let px_count = out.len().min(layer.len()) / 4;
        for i in 0..px_count {
            let dst = i * 4;
            let src_a = layer[dst + 3] as f32 / 255.0;
            let inv_a = 1.0 - src_a;
            out[dst] = (layer[dst] as f32 * src_a + out[dst] as f32 * inv_a) as u8;
            out[dst + 1] = (layer[dst + 1] as f32 * src_a + out[dst + 1] as f32 * inv_a) as u8;
            out[dst + 2] = (layer[dst + 2] as f32 * src_a + out[dst + 2] as f32 * inv_a) as u8;
            out[dst + 3] = ((src_a + out[dst + 3] as f32 / 255.0 * inv_a) * 255.0) as u8;
        }
    }
    Some((width, height, out))
}

impl WowDataAdapter for BenillaAdapter {
    fn source_label(&self) -> &'static str {
        BENILLA_PINNED_COMMIT
    }

    fn validate_client(&self) -> Result<ClientBuildInfo> {
        if !self.data_path.exists() {
            bail!("WoW data path does not exist: {}", self.data_path.display());
        }
        let archives = self.open_archives()?;
        if !archives.has_mpq_hint() {
            bail!(
                "expected Vanilla MPQ files in {}, such as patch.MPQ/common.MPQ",
                self.data_path.display()
            );
        }
        Ok(ClientBuildInfo {
            build: SUPPORTED_BUILD,
            version_string: "1.12.1".to_string(),
        })
    }

    fn summarize_data(&self) -> Result<WowDataSummary> {
        Ok(WowDataSummary {
            adapter: self.source_label().to_string(),
            tables: vec![
                "CharSections.dbc".to_string(),
                "CharHairGeosets.dbc".to_string(),
                "CharacterFacialHairStyles.dbc".to_string(),
                "ItemDisplayInfo.dbc".to_string(),
                "HelmetGeosetVisData.dbc".to_string(),
            ],
            notes: vec![
                "uses real MPQ archive reads with full DBC row decoding for CharSections and ItemDisplayInfo"
                    .to_string(),
                "build guard enforced for WoW 1.12.1 (5875)".to_string(),
                "high elf currently aliases to night elf female until a dedicated dataset is integrated"
                    .to_string(),
            ],
        })
    }

    fn load_character_source(&self, cfg: &ExportConfig) -> Result<CharacterSourceData> {
        let resolved = self.resolve_profile(cfg)?;
        let base_model = self.read_model_asset(&resolved.base_model_path)?;
        let char_sections_table = self.read_dbc_table("CharSections")?;
        let hair_geosets_table = self.read_dbc_table("CharHairGeosets")?;
        let facial_styles_table = self.read_dbc_table("CharacterFacialHairStyles")?;
        let item_display_table = self.read_dbc_table("ItemDisplayInfo")?;
        let sections = decode_char_sections(&char_sections_table);
        let hair_geosets = decode_char_hair_geosets(&hair_geosets_table);
        let facial_styles = decode_character_facial_hair_styles(&facial_styles_table);
        let item_displays = decode_item_display_info(&item_display_table);

        let hair_geoset = resolve_hair_geoset(
            &hair_geosets,
            resolved.race_id,
            resolved.gender_id,
            i32::from(cfg.profile.hair_style_index),
        );
        let facial_geoset = resolve_facial_geoset(
            &facial_styles,
            resolved.race_id,
            resolved.gender_id,
            i32::from(cfg.profile.facial_hair_index),
        );
        let skin_variant = 100 + (u16::from(cfg.profile.skin_index) % 32);
        let m2_view_geoset = 1000 + (base_model.model.header.views.count as u16 % 64);

        let skin_row = Self::pick_section(
            &sections,
            resolved.race_id,
            resolved.gender_id,
            0,
            i32::from(cfg.profile.skin_index),
            0,
        );
        let face_row = Self::pick_section(
            &sections,
            resolved.race_id,
            resolved.gender_id,
            1,
            i32::from(cfg.profile.skin_index),
            i32::from(cfg.profile.face_index),
        );
        let hair_row = Self::pick_section(
            &sections,
            resolved.race_id,
            resolved.gender_id,
            3,
            i32::from(cfg.profile.hair_color_index),
            i32::from(cfg.profile.hair_style_index),
        );
        let facial_row = Self::pick_section(
            &sections,
            resolved.race_id,
            resolved.gender_id,
            2,
            i32::from(cfg.profile.hair_color_index),
            i32::from(cfg.profile.facial_hair_index),
        );

        let mut body_textures = Vec::new();
        body_textures.extend(Self::collect_section_textures(skin_row));
        body_textures.extend(Self::collect_section_textures(face_row));
        body_textures.extend(Self::collect_section_textures(hair_row));
        body_textures.extend(Self::collect_section_textures(facial_row));
        if body_textures.is_empty() {
            bail!(
                "no CharSections textures resolved for race_id={} gender_id={}",
                resolved.race_id,
                resolved.gender_id
            );
        }
        let body_layers_decoded = body_textures
            .iter()
            .filter_map(|path| self.read_blp_rgba(path))
            .collect::<Vec<_>>();
        let composited_body_rgba = composite_layers(&body_layers_decoded);

        let mut geoset_ids = vec![skin_variant, hair_geoset, facial_geoset, m2_view_geoset];

        let mut equipment = BTreeMap::new();
        let mut equipment_models = BTreeMap::new();
        let mut equipment_textures_rgba = BTreeMap::new();
        for (slot, display_id) in &cfg.equipment_display_ids {
            if *display_id == 0 {
                continue;
            }
            let Some(display) = item_displays.get(display_id) else {
                continue;
            };
            let (model_path, texture_path) =
                Self::pick_item_asset(display, slot.eq_ignore_ascii_case("offhand"));
            let Some(model_path) = model_path else {
                continue;
            };
            equipment.insert(
                slot.clone(),
                EquipmentRecipe {
                    model_path: model_path.clone(),
                    texture_path,
                    attachment: Self::attachment_for_slot(slot).to_string(),
                    geoset_groups: [
                        display.geoset_group_1,
                        display.geoset_group_2,
                        display.geoset_group_3,
                    ],
                },
            );
            for group in [display.geoset_group_1, display.geoset_group_2, display.geoset_group_3]
                .into_iter()
                .flatten()
            {
                if group > 0 {
                    geoset_ids.push(group as u16);
                }
            }
            if let Ok(asset) = self.read_model_asset(&model_path) {
                equipment_models.insert(slot.clone(), asset);
            }
            if let Some(tex) = equipment
                .get(slot)
                .and_then(|r| r.texture_path.as_ref())
                .and_then(|path| self.read_blp_rgba(path))
            {
                equipment_textures_rgba.insert(slot.clone(), tex);
            }
        }
        let equipment_rows_resolved = equipment.len();
        geoset_ids.sort_unstable();
        geoset_ids.dedup();
        let base_version = base_model.model.header.version;
        let base_anim_count = base_model.model.animations.len();
        let base_bone_count = base_model.model.bones.len();
        let base_vert_count = base_model.model.vertices.len();
        let base_view_count = base_model.model.header.views.count;

        let appearance = AppearanceRecipe {
            geoset_ids,
            body_texture_layers: body_textures,
        };

        Ok(CharacterSourceData {
            base_model,
            profile: cfg.profile.clone(),
            appearance,
            composited_body_rgba,
            equipment,
            equipment_models,
            equipment_textures_rgba,
            requested_animations: cfg.animation_ids.clone(),
            wow_data_path: cfg.wow_data_path.clone(),
            adapter_notes: vec![
                format!(
                    "resolved_profile={}::{:?}",
                    resolved.canonical_race, resolved.sex
                ),
                format!("requested_profile={}", resolved.normalized_race),
                format!("build={SUPPORTED_BUILD}"),
                format!(
                    "m2_header=path:{} version:{} animations:{} bones:{} vertices:{} views:{}",
                    resolved.base_model_path,
                    base_version,
                    base_anim_count,
                    base_bone_count,
                    base_vert_count,
                    base_view_count,
                ),
                format!(
                    "dbc_records=CharSections:{} CharHairGeosets:{} CharacterFacialHairStyles:{} ItemDisplayInfo:{}",
                    char_sections_table.header.record_count,
                    hair_geosets_table.header.record_count,
                    facial_styles_table.header.record_count,
                    item_display_table.header.record_count
                ),
                format!(
                    "dbc_layout=CharSections(fields:{} recSize:{} strSize:{})",
                    char_sections_table.header.field_count,
                    char_sections_table.header.record_size,
                    char_sections_table.header.string_block_size
                ),
                format!(
                    "section_flags=skin:{:?} face:{:?} hair:{:?} facial:{:?}",
                    skin_row.and_then(|r| r.flags),
                    face_row.and_then(|r| r.flags),
                    hair_row.and_then(|r| r.flags),
                    facial_row.and_then(|r| r.flags)
                ),
                format!("equipment_rows_resolved={}", equipment_rows_resolved),
            ]
            .into_iter()
            .chain(resolved.notes)
            .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::{CharacterProfile, Sex};

    fn sample_cfg(race: &str, sex: Sex) -> ExportConfig {
        ExportConfig {
            wow_data_path: PathBuf::from("B:\\raven\\RavenCraft\\Data"),
            output_path: PathBuf::from("out.glb"),
            profile: CharacterProfile {
                race: race.to_string(),
                sex,
                skin_index: 2,
                face_index: 1,
                hair_style_index: 4,
                hair_color_index: 3,
                facial_hair_index: 0,
            },
            equipment_display_ids: BTreeMap::from([("head".to_string(), 111u32)]),
            animation_ids: vec![0, 4, 5],
        }
    }

    #[test]
    fn supports_vanilla_profiles() {
        let cfg = sample_cfg("human", Sex::Male);
        let adapter = BenillaAdapter::new(&cfg.wow_data_path);
        let source = adapter.resolve_profile(&cfg).expect("human male profile");
        assert!(source.base_model_path.contains("HumanMale"));
    }

    #[test]
    fn aliases_high_elf_to_night_elf_female() {
        let cfg = sample_cfg("high elf", Sex::Female);
        let adapter = BenillaAdapter::new(&cfg.wow_data_path);
        let source = adapter.resolve_profile(&cfg).expect("alias profile");
        assert!(source
            .notes
            .iter()
            .any(|note| note.contains("high elf alias")));
    }

    #[test]
    fn rejects_unsupported_profile() {
        let cfg = sample_cfg("bloodelf", Sex::Female);
        let adapter = BenillaAdapter::new(&cfg.wow_data_path);
        let err = adapter.resolve_profile(&cfg).expect_err("should fail");
        assert!(err.to_string().contains("unsupported race"));
    }
}
