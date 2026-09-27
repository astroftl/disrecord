use serde::{Deserialize, Serialize};
use serenity::all::{ApplicationId, GuildId};
use std::collections::HashMap;
use std::path::PathBuf;
use serde_with::skip_serializing_none;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MixType {
    Exclude(Vec<String>),
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OutputConfig {
    pub zip_files: Option<bool>,
    pub embed_files: Option<bool>,
    pub post_cmd_per_user: Option<String>,
    pub post_cmd_per_mix: Option<String>,
    pub post_cmd_zip: Option<String>,
    pub post_cmd_all: Option<String>,
    pub custom_mixes: Option<HashMap<String, Vec<MixType>>>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub bot_token: String,
    pub app_id: ApplicationId,
    pub record_dir: PathBuf,
    pub date_format: String,
    pub default_output: OutputConfig,
    pub server_output: HashMap<GuildId, OutputConfig>,
}

impl AppConfig {
    pub fn get_output(&self, guild_id: &GuildId) -> &OutputConfig {
        match self.server_output.get(guild_id) {
            None => &self.default_output,
            Some(output) => output,
        }
    }
}