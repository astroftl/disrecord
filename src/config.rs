use serde::{Deserialize, Serialize};
use serenity::all::{ApplicationId, GuildId};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputConfig {
    pub embed_files: Option<bool>,
    pub post_command: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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