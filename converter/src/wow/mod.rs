mod benilla_adapter;
pub mod archive;
pub mod blp;
pub mod binary;
pub mod dbc;
pub mod m2;
pub mod skin;
pub mod types;

use anyhow::Result;
use serde::Serialize;

use crate::config::ExportConfig;
use crate::wow::benilla_adapter::BenillaAdapter;
use crate::wow::types::{CharacterSourceData, ClientBuildInfo};

#[derive(Debug, Clone, Serialize)]
pub struct WowDataSummary {
    pub adapter: String,
    pub tables: Vec<String>,
    pub notes: Vec<String>,
}

pub trait WowDataAdapter {
    fn source_label(&self) -> &'static str;
    fn validate_client(&self) -> Result<ClientBuildInfo>;
    fn summarize_data(&self) -> Result<WowDataSummary>;
    fn load_character_source(&self, cfg: &ExportConfig) -> Result<CharacterSourceData>;
}

pub struct DataPipeline {
    adapter: Box<dyn WowDataAdapter>,
}

impl DataPipeline {
    pub fn for_config(cfg: &ExportConfig) -> Self {
        let adapter = BenillaAdapter::new(&cfg.wow_data_path);
        Self {
            adapter: Box::new(adapter),
        }
    }

    pub fn validate_client(&self) -> Result<ClientBuildInfo> {
        self.adapter.validate_client()
    }

    pub fn summarize_data(&self) -> Result<WowDataSummary> {
        self.adapter.summarize_data()
    }

    pub fn load_character_source(&self, cfg: &ExportConfig) -> Result<CharacterSourceData> {
        self.adapter.load_character_source(cfg)
    }
}
