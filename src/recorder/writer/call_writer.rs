use std::collections::HashSet;
use crate::recorder::writer::stream_writer::StreamWriter;
use crate::recorder::writer::VoiceUpdateType;
use crate::recorder::{RecordingMetadata, RecordingSummary};
use dashmap::{DashMap, DashSet};
use serenity::all::{GuildId, UserId};
use std::sync::{Arc, Mutex};
use chrono::Utc;
use tokio::sync::oneshot::channel;
use crate::config::{AppConfig, MixType};
use crate::recorder::writer::mixer::mix_files;
use crate::recorder::writer::zipper::zip_files;

#[derive(Debug)]
pub struct CallWriter {
    metadata: RecordingMetadata,
    config: Arc<AppConfig>,
    streams: DashMap<UserId, Arc<StreamWriter>>,
    known_users: DashSet<UserId>,
    tick_count: Mutex<usize>,
}

impl CallWriter {
    pub fn new(metadata: RecordingMetadata, config: Arc<AppConfig>) -> Self {
        Self {
            metadata,
            config,
            streams: DashMap::new(),
            known_users: DashSet::new(),
            tick_count: Mutex::new(0),
        }
    }

    pub async fn push(&self, update_data: VoiceUpdateType) {
        match update_data {
            VoiceUpdateType::VcUpdate(opus_update) => {
                let tick_count = {
                    let mut counter = self.tick_count.lock().unwrap();
                    *counter += 1;
                    *counter
                };

                let silent_users = self.known_users.clone();

                for opus_update in opus_update {
                    let user = opus_update.user.clone();

                    let stream = match self.streams.get(&user) {
                        Some(stream) => stream.clone(),
                        None => {
                            warn!("[{}] Got Opus update for a user we don't know about yet: {user}", self.metadata.guild_id);
                            continue;
                        }
                    };

                    silent_users.remove(&user);
                    stream.push(opus_update.opus_data.as_slice(), tick_count).await;
                }

                for user in silent_users {
                    let stream = self.streams.get(&user).unwrap();
                    stream.push_silence(tick_count).await;
                }
            }
            VoiceUpdateType::User(user_update) => {
                let user = user_update.user;

                if !self.streams.contains_key(&user) {
                    trace!("[{}] <{}> Got connection from new user!", self.metadata.guild_id, user);
                    let tick_count = *self.tick_count.lock().unwrap();
                    let new_stream = StreamWriter::new(self.metadata.guild_id, user, user_update.username, self.metadata.output_dir.clone(), tick_count).await;
                    match new_stream {
                        None => {
                            error!("[{}] <{}> Failed to create new stream!", self.metadata.guild_id, user);
                            return;
                        }
                        Some(new_stream) => {
                            self.streams.insert(user, new_stream);
                            self.known_users.insert(user);
                        }
                    }
                } else {
                    debug!("[{}] <{}> Got reconnect from existing user!", self.metadata.guild_id, user);
                }
            }
        }
    }

    pub async fn finish(&self) -> Option<RecordingSummary> {
        debug!("[{}] Finishing CallWriter!", self.metadata.guild_id);

        let mut stream_files = Vec::new();

        for stream in &self.streams {
            stream.finish().await;
            stream_files.push(stream.filepath());
        }

        self.streams.clear();

        let mut known_users = HashSet::new();
        for user in self.known_users.iter() {
            known_users.insert(*user);
        }

        let mut mix_rxs = Vec::new();

        let (mix_tx, mix_rx) = channel();
        let mix_path = self.metadata.output_dir.clone();
        let mix_output = format!("{}.opus", self.metadata.output_dir_name);
        let mix_guild_id = self.metadata.guild_id.clone();
        let mix_input_files = stream_files.clone();
        tokio::spawn(async move {
            mix_files(&mix_input_files, mix_path, mix_output, mix_guild_id, mix_tx).await;
        });
        mix_rxs.push(mix_rx);

        if let Some(custom_mixes) = &self.config.get_output(&self.metadata.guild_id).custom_mixes {
            for (custom_mix_name, custom_mix_type) in custom_mixes {
                debug!("Performing custom mix {custom_mix_name}: {custom_mix_type:?}");
                let (custom_mix_tx, custom_mix_rx) = channel();
                let custom_mix_path = self.metadata.output_dir.clone();
                let custom_mix_output = format!("{custom_mix_name}-{}.opus", self.metadata.output_dir_name);
                let custom_mix_guild_id = self.metadata.guild_id.clone();
                let mut custom_mix_input_files = stream_files.clone();

                for custom_type in custom_mix_type {
                    match custom_type {
                        MixType::Exclude(names) => {
                            for name in names {
                                custom_mix_input_files.retain(|x| {
                                    x.file_name().unwrap().ne("{name}.opus")
                                });
                            }
                        }
                    }
                }

                tokio::spawn(async move {
                    mix_files(&custom_mix_input_files, custom_mix_path, custom_mix_output, custom_mix_guild_id, custom_mix_tx).await;
                });
                mix_rxs.push(custom_mix_rx);
            }
        }

        let zip_rx = if self.config.get_output(&self.metadata.guild_id).zip_files.unwrap_or(false) {
            let (zip_tx, zip_rx) = channel();
            let zip_path = self.metadata.output_dir.clone();
            let zip_output = format!("{}.zip", self.metadata.output_dir_name);
            let zip_guild_id = self.metadata.guild_id.clone();
            let zip_input_files = stream_files.clone();
            tokio::spawn(async move {
                zip_files(&zip_input_files, zip_path, zip_output, zip_guild_id, zip_tx).await;
            });
            Some(zip_rx)
        } else {
            None
        };

        Some(RecordingSummary {
            started: self.metadata.started.clone(),
            ended: Utc::now(),
            known_users,
            stream_files,
            zip_rx,
            mix_rxs,
        })
    }
}

impl Drop for CallWriter {
    fn drop(&mut self) {
        trace!("[{}] CallWriter::drop", self.metadata.guild_id);
    }
}