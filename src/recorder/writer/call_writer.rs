use crate::recorder::writer::stream_writer::StreamWriter;
use crate::recorder::writer::RtpUpdate;
use crate::recorder::RecordingMetadata;
use dashmap::DashMap;
use serenity::all::UserId;
use std::sync::Arc;

#[derive(Debug)]
pub struct CallWriter {
    metadata: RecordingMetadata,
    streams: DashMap<UserId, Arc<StreamWriter>>
}

impl CallWriter {
    pub fn new(metadata: RecordingMetadata) -> Self {
        Self {
            metadata,
            streams: DashMap::new(),
        }
    }

    pub async fn push(&self, rtp_update: RtpUpdate) {
        let stream = match self.streams.get(&rtp_update.user) {
            Some(stream) => stream.clone(),
            None => {
                let new_stream = StreamWriter::new(self.metadata.guild_id, rtp_update.user, self.metadata.output_dir.clone()).await;
                match new_stream {
                    None => {
                        error!("[{}] <{}> Failed to create new stream!", self.metadata.guild_id, rtp_update.user);
                        return;
                    }
                    Some(new_stream) => {
                        let new_stream = Arc::new(new_stream);
                        self.streams.insert(rtp_update.user, new_stream.clone());
                        new_stream
                    }
                }
            }
        };

        stream.push(rtp_update).await;
    }

    pub async fn finish(&self) {
        debug!("[{}] Finishing CallWriter!", self.metadata.guild_id);
        for stream in &self.streams {
            stream.finish().await;
        }

        self.streams.clear();
    }
}

impl Drop for CallWriter {
    fn drop(&mut self) {
        trace!("[{}] CallWriter::drop", self.metadata.guild_id);
    }
}