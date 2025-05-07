use std::collections::HashSet;
use crate::recorder::writer::stream_writer::StreamWriter;
use crate::recorder::writer::VoiceUpdateType;
use crate::recorder::{RecordingMetadata, RecordingSummary};
use dashmap::{DashMap, DashSet};
use serenity::all::UserId;
use std::sync::{Arc, Mutex};
use chrono::Utc;
use tokio::sync::oneshot::channel;
use crate::recorder::writer::zipper::zip_files;

#[derive(Debug)]
pub struct CallWriter {
    metadata: RecordingMetadata,
    streams: DashMap<UserId, Arc<StreamWriter>>,
    known_users: DashSet<UserId>,
    tick_count: Mutex<usize>,
}

impl CallWriter {
    pub fn new(metadata: RecordingMetadata) -> Self {
        Self {
            metadata,
            streams: DashMap::new(),
            known_users: DashSet::new(),
            tick_count: Mutex::new(0),
        }
    }

    pub async fn push(&self, update_data: VoiceUpdateType) {
        match update_data {
            VoiceUpdateType::Opus(opus_update) => {
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

                let new_stream = StreamWriter::new(self.metadata.guild_id, user, user_update.username, self.metadata.output_dir.clone()).await;
                match new_stream {
                    None => {
                        error!("[{}] <{}> Failed to create new stream!", self.metadata.guild_id, user);
                        return;
                    }
                    Some(new_stream) => {
                        let new_stream = Arc::new(new_stream);

                        let tick_count = *self.tick_count.lock().unwrap();
                        new_stream.fill_silence(tick_count).await;

                        self.streams.insert(user, new_stream.clone());
                        self.known_users.insert(user);
                    }
                }
            }
        }
    }

    pub async fn finish(&self) -> Option<RecordingSummary> {
        debug!("[{}] Finishing CallWriter!", self.metadata.guild_id);
        for stream in &self.streams {
            stream.finish().await;
        }

        self.streams.clear();

        let mut known_users = HashSet::new();
        for user in self.known_users.iter() {
            known_users.insert(*user);
        }

        let (zip_tx, zip_rx) = channel();

        let zip_path = self.metadata.output_dir.clone();
        let zip_name = format!("{}.zip", self.metadata.output_dir_name);
        let zip_guild_id = self.metadata.guild_id.clone();
        tokio::spawn(async move {
            zip_files(zip_path, zip_name, zip_guild_id, zip_tx).await;
        });

        Some(RecordingSummary {
            started: self.metadata.started.clone(),
            ended: Utc::now(),
            known_users,
            zip_rx,
        })
    }
}

impl Drop for CallWriter {
    fn drop(&mut self) {
        trace!("[{}] CallWriter::drop", self.metadata.guild_id);
    }
}