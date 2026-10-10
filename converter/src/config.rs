use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Male,
    Female,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterProfile {
    pub race: String,
    pub sex: Sex,
    pub skin_index: u8,
    pub face_index: u8,
    pub hair_style_index: u8,
    pub hair_color_index: u8,
    pub facial_hair_index: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub wow_data_path: PathBuf,
    pub output_path: PathBuf,
    pub profile: CharacterProfile,
    #[serde(default)]
    pub equipment_display_ids: BTreeMap<String, u32>,
    #[serde(default)]
    pub animation_ids: Vec<u16>,
}

impl ExportConfig {
    pub fn from_json_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let parsed: ExportConfig = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse config JSON: {}", path.display()))?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn validate(&self) -> Result<()> {
        if self.profile.race.trim().is_empty() {
            bail!("profile.race cannot be empty");
        }

        if self.profile.skin_index > 31
            || self.profile.face_index > 31
            || self.profile.hair_style_index > 31
            || self.profile.hair_color_index > 31
            || self.profile.facial_hair_index > 31
        {
            bail!("appearance indexes must be between 0 and 31");
        }

        if !self.wow_data_path.exists() {
            bail!(
                "wow_data_path does not exist: {}",
                self.wow_data_path.display()
            );
        }

        if !self.wow_data_path.is_dir() {
            bail!(
                "wow_data_path must be a directory: {}",
                self.wow_data_path.display()
            );
        }

        if let Some(parent) = self.output_path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            bail!("output directory does not exist: {}", parent.display());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> ExportConfig {
        ExportConfig {
            wow_data_path: PathBuf::from("."),
            output_path: PathBuf::from("out.glb"),
            profile: CharacterProfile {
                race: "human".to_string(),
                sex: Sex::Male,
                skin_index: 2,
                face_index: 1,
                hair_style_index: 4,
                hair_color_index: 3,
                facial_hair_index: 0,
            },
            equipment_display_ids: BTreeMap::new(),
            animation_ids: vec![0, 4, 5],
        }
    }

    #[test]
    fn validation_rejects_bad_indexes() {
        let mut cfg = sample_config();
        cfg.profile.skin_index = 99;
        let err = cfg.validate().expect_err("should fail");
        assert!(err.to_string().contains("appearance indexes"));
    }
}
