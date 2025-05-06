mod call_writer;
mod stream_writer;
mod muxer;

use crate::recorder::writer::call_writer::CallWriter;
use crate::recorder::{RecorderConfig, RecordingMetadata};
use chrono::Utc;
use dashmap::DashMap;
use serenity::all::{GuildId, UserId};
use songbird::packet::wrap::{Wrap16, Wrap32};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, PartialEq)]
pub struct RtpUpdate {
    pub guild: GuildId,
    pub user: UserId,
    pub ssrc: u32,
    pub timestamp: Wrap32,
    pub sequence: Wrap16,
    pub opus_data: Vec<u8>,
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
            started,
        };

        self.calls.insert(guild_id, Arc::new(CallWriter::new(rec_metadata)));
    }

    pub async fn finish(&self, guild_id: GuildId) {
        let call = self.calls.remove(&guild_id);
        match call {
            None => {
                error!("Tried to finish non-existent recording for guild: {guild_id}");
            }
            Some(call) => {
                call.1.finish().await;
            }
        }
    }

    pub fn run(writer: Arc<Self>, mut rtp_rx: mpsc::Receiver<RtpUpdate>) {
        tokio::spawn(async move {
            while let Some(rtp_update) = rtp_rx.recv().await {
                match writer.calls.get(&rtp_update.guild) {
                    Some(call) => {
                        call.push(rtp_update).await;
                    }
                    None => {
                        error!("Received voice data for guild without call: {}", rtp_update.guild);
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