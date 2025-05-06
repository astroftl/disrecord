use std::collections::HashSet;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serenity::all::{GuildId, UserId};

mod voice_receiver;
mod writer;
pub mod recorder;

#[derive(Clone, Debug)]
pub struct RecordingMetadata {
    pub guild_id: GuildId,
    pub output_dir: PathBuf,
    pub started: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RecordingSummary {
    pub guild_id: GuildId,
    pub output_dir: PathBuf,
    pub started: DateTime<Utc>,
    pub known_users: HashSet<UserId>,
}

#[derive(Clone, Debug)]
pub struct RecorderConfig {
    pub base_dir: PathBuf,
    pub subdir_fmt:  String,
}