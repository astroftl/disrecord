use std::collections::HashSet;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serenity::all::{GuildId, UserId};
use tokio::sync::oneshot::Receiver;

mod voice_receiver;
mod writer;
pub mod recorder;

#[derive(Clone, Debug)]
pub struct RecordingMetadata {
    pub guild_id: GuildId,
    pub output_dir: PathBuf,
    pub output_dir_name: String,
    pub started: DateTime<Utc>,
}

#[derive(Debug)]
pub struct RecordingSummary {
    pub started: DateTime<Utc>,
    pub ended: DateTime<Utc>,
    pub known_users: HashSet<UserId>,
    pub zip_rx: Receiver<Result<PathBuf, String>>,
}

#[derive(Clone, Debug)]
pub struct RecorderConfig {
    pub base_dir: PathBuf,
    pub subdir_fmt:  String,
}