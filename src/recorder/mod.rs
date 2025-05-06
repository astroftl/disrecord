use std::collections::HashSet;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use rkyv::{Archive, Deserialize, Serialize};
use serenity::all::{GuildId, UserId};
use songbird::packet::wrap::{Wrap16, Wrap32};

mod voice_receiver;
mod voice_writer;
pub mod record_manager;

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
pub struct RecordConfig {
    pub base_dir: PathBuf,
    pub subdir_fmt:  String,
}

#[derive(Debug, PartialEq)]
pub struct RtpUpdate {
    pub user: UserId,
    pub ssrc: u32,
    pub timestamp: Wrap32,
    pub sequence: Wrap16,
    pub opus_data: Vec<u8>,
}

#[derive(Debug)]
pub enum VoiceUpdate {
    Rtp(RtpUpdate),
}

#[derive(Archive, Deserialize, Serialize, Debug, PartialEq)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct RtpSave {
    pub timestamp: u32,
    pub sequence: u16,
    pub opus_data: Vec<u8>,
}