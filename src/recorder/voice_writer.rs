use dashmap::DashMap;
use serenity::all::{GuildId, UserId};
use std::path::PathBuf;
use std::sync::Arc;
use rkyv::{rancor, to_bytes};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex};
use crate::recorder::{RecordingMetadata, RtpSave, VoiceUpdate};

#[derive(Clone, Debug)]
pub struct VoiceWriter {
    guild_id: GuildId,
    output_dir: PathBuf,
    files: DashMap<UserId, Arc<Mutex<File>>>,
}

impl VoiceWriter {
    pub fn new(metadata: RecordingMetadata) -> Self {
        Self {
            guild_id: metadata.guild_id,
            output_dir: metadata.output_dir,
            files: DashMap::new(),
        }
    }

    async fn get_or_open_file(&self, user: UserId) -> Option<Arc<Mutex<File>>> {
        match self.files.get_mut(&user) {
            None => {
                let filepath =  self.output_dir.join(user.to_string() + ".rec");

                if let Some(parent) = filepath.parent() {
                    if let Err(e) = tokio::fs::create_dir_all(parent).await {
                        error!("[{}] <{user}> Failed to create parent directory: {e:?}", self.guild_id);
                        return None;
                    }
                }

                debug!("[{}] Creating file {filepath:?}", self.guild_id);
                match File::create(filepath).await {
                    Ok(file) => {
                        let file = Arc::new(Mutex::new(file));
                        self.files.insert(user, file.clone());
                        Some(file)
                    }
                    Err(e) => {
                        error!("[{}] <{user}> Failed to create file: {e:?}", self.guild_id);
                        None
                    }
                }
            }
            Some(file) => Some(file.clone()),
        }
    }

    async fn process_voice_data(&self, data: VoiceUpdate) {
        match data {
            VoiceUpdate::Rtp(rtp_update) => {
                let user = rtp_update.user;
                if let Some(file) =  self.get_or_open_file(user).await {
                    let rtp_save = RtpSave {
                        timestamp: rtp_update.timestamp.into(),
                        sequence: rtp_update.sequence.into(),
                        opus_data: rtp_update.opus_data,
                    };

                    let rtp_bytes = to_bytes::<rancor::Error>(&rtp_save).unwrap();

                    let mut file_lock = file.lock().await;
                    file_lock.write_all(rtp_bytes.as_slice()).await.unwrap();
                }
            }
        }
    }

    pub fn run(recorder: Arc<Mutex<Self>>, mut voice_rx: mpsc::Receiver<VoiceUpdate>) {
        tokio::spawn(async move {
            while let Some(voice_data) = voice_rx.recv().await {
                recorder.lock().await.process_voice_data(voice_data).await;
            }
        });
    }
}

impl Drop for VoiceWriter {
    fn drop(&mut self) {
        trace!("VoiceWriter::drop");
    }
}