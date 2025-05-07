mod call_writer;
mod stream_writer;
mod muxer;
mod zipper;

use crate::recorder::writer::call_writer::CallWriter;
use crate::recorder::{RecorderConfig, RecordingMetadata, RecordingSummary};
use chrono::Utc;
use dashmap::DashMap;
use serenity::all::{GuildId, UserId};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, PartialEq)]
pub struct OpusUpdate {
    pub user: UserId,
    pub opus_data: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct UserUpdate {
    pub user: UserId,
    pub username: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum VoiceUpdateType {
    Opus(Vec<OpusUpdate>),
    User(UserUpdate),
}

#[derive(Debug, PartialEq)]
pub struct VoiceUpdate {
    pub guild: GuildId,
    pub update: VoiceUpdateType,
}

#[derive(Debug)]
pub struct Writer {
    config: RecorderConfig,
    calls: DashMap<GuildId, Arc<CallWriter>>,
}

impl Writer {
    pub fn new(config: RecorderConfig) -> Self {
        Self {
            config,
            calls: DashMap::new(),
        }
    }

    pub fn start(&self, guild_id: GuildId) {
        let started = Utc::now();
        let output_dir_name = started.format(self.config.subdir_fmt.as_str()).to_string();
        let output_dir = self.config.base_dir.join(format!("{}", guild_id)).join(output_dir_name.as_str());

        let rec_metadata = RecordingMetadata {
            guild_id,
            output_dir,
            output_dir_name,
            started,
        };

        self.calls.insert(guild_id, Arc::new(CallWriter::new(rec_metadata)));
    }

    pub async fn finish(&self, guild_id: GuildId) -> Option<RecordingSummary> {
        let call = self.calls.remove(&guild_id);
        match call {
            None => {
                error!("Tried to finish non-existent recording for guild: {guild_id}");
                None
            }
            Some((_, call)) => {
                call.finish().await
            }
        }
    }

    pub fn run(writer: Arc<Self>, mut voice_rx: mpsc::Receiver<VoiceUpdate>) {
        tokio::spawn(async move {
            while let Some(voice_update) = voice_rx.recv().await {
                match writer.calls.get(&voice_update.guild) {
                    Some(call) => {
                        call.push(voice_update.update).await;
                    }
                    None => {
                        error!("Received voice data for guild without call: {}", voice_update.guild);
                    }
                }
            }
        });
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        trace!("VoiceWriter::drop");
    }
}