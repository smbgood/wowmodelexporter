use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::config::{CharacterProfile, Sex};
use crate::wow::m2::M2Model;
use crate::wow::skin::SkinData;

#[derive(Debug, Clone, Serialize)]
pub struct ClientBuildInfo {
    pub build: u16,
    pub version_string: String,
}

#[derive(Debug, Clone)]
pub struct AppearanceRecipe {
    pub geoset_ids: Vec<u16>,
    pub body_texture_layers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EquipmentRecipe {
    pub model_path: String,
    pub texture_path: Option<String>,
    pub attachment: String,
    pub geoset_groups: [Option<i32>; 3],
}

#[derive(Debug, Clone)]
pub struct ModelAsset {
    pub model_path: String,
    pub model: M2Model,
    pub skin: Option<SkinData>,
}

#[derive(Debug, Clone)]
pub struct CharacterSourceData {
    pub base_model: ModelAsset,
    pub profile: CharacterProfile,
    pub appearance: AppearanceRecipe,
    pub composited_body_rgba: Option<(u32, u32, Vec<u8>)>,
    pub equipment: BTreeMap<String, EquipmentRecipe>,
    pub equipment_models: BTreeMap<String, ModelAsset>,
    pub equipment_textures_rgba: BTreeMap<String, (u32, u32, Vec<u8>)>,
    pub requested_animations: Vec<u16>,
    pub wow_data_path: PathBuf,
    pub adapter_notes: Vec<String>,
}

impl CharacterSourceData {
    pub fn race_sex_key(&self) -> String {
        let sex = match self.profile.sex {
            Sex::Male => "male",
            Sex::Female => "female",
        };
        format!("{}_{}", self.profile.race.to_lowercase(), sex)
    }
}
