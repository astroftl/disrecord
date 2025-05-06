use crate::recorder::voice_receiver::VoiceReceiver;
use crate::recorder::{RecordConfig, RecordingMetadata, RecordingSummary};
use chrono::Utc;
use dashmap::DashMap;
use serenity::all::{ChannelId, Context, GuildId};
use songbird::CoreEvent;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug)]
pub struct RecordManager {
    config: RecordConfig,
    voice_handlers: DashMap<GuildId, VoiceReceiver>,
}

impl RecordManager {
    pub fn new(config: RecordConfig) -> Self {
        Self {
            config,
            voice_handlers: DashMap::new(),
        }
    }

    pub async fn get(ctx: &Context) -> Option<Arc<Self>> {
        let data = ctx.data.read().await;
        data.get::<Self>().cloned()
    }

    pub async fn has_call(ctx: &Context, guild_id: GuildId) -> bool {
        let sbird = songbird::get(ctx).await.expect("Songbird doesn't exist!");
        sbird.get(guild_id).is_some()
    }

    pub async fn join(&self, ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> Result<(), String> {
        trace!("[{guild_id}] Joining: {channel_id}");

        let sbird = songbird::get(ctx).await.expect("Songbird doesn't exist!");

        // Some events relating to voice receive fire *while joining*.
        // We must make sure that any event handlers are installed before we attempt to join.
        if sbird.get(guild_id).is_none() {
            let started = Utc::now();
            let output_dir_name = started.format(self.config.subdir_fmt.as_str()).to_string();
            let output_dir = self.config.base_dir.join(format!("{}", guild_id)).join(output_dir_name.as_str());

            let call_lock = sbird.get_or_insert(guild_id);
            let mut call = call_lock.lock().await;

            let rec_metadata = RecordingMetadata {
                guild_id,
                output_dir,
                started,
            };

            let voice_receiver = VoiceReceiver::new(rec_metadata).await;

            call.add_global_event(CoreEvent::RtpPacket.into(), voice_receiver.clone());
            call.add_global_event(CoreEvent::RtcpPacket.into(), voice_receiver.clone());
            call.add_global_event(CoreEvent::ClientDisconnect.into(), voice_receiver.clone());
            call.add_global_event(CoreEvent::SpeakingStateUpdate.into(), voice_receiver.clone());

            self.voice_handlers.insert(guild_id, voice_receiver);
        }

        // TODO: Check that channel is in the guild and that the bot has access to it before joining.

        if let Err(e) = sbird.join(guild_id, channel_id).await {
            error!("[{guild_id}] Failed to join voice channel: {e:?}");

            // Although we failed to join, we need to clear out existing event handlers on the call.
            _ = sbird.remove(guild_id).await;
            self.voice_handlers.remove(&guild_id);

            Err(format!("Failed to join voice channel: {e}"))
        } else {
            info!("[{guild_id}] Joined channel {channel_id} and began recording!");
            Ok(())
        }
    }

    pub async fn rejoin(&self, ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> Result<(), String> {
        trace!("[{guild_id}] Re-joining: {channel_id}");

        let sbird = songbird::get(ctx).await.expect("Songbird doesn't exist!");

        if sbird.get(guild_id).is_some() {
            let old_channel_id = {
                let call = sbird.get(guild_id).unwrap();
                ChannelId::from(call.lock().await.current_channel().unwrap().0)
            };

            if old_channel_id == channel_id {
                if let Err(e) = sbird.leave(guild_id).await {
                    error!("[{guild_id}] Failed to leave voice channel: {e:?}");

                    // Although we failed to join, we need to clear out existing event handlers on the call.
                    _ = sbird.remove(guild_id).await;
                    self.voice_handlers.remove(&guild_id);

                    return Err(format!("Failed to leave voice channel: {e}"))
                };

                sleep(Duration::from_millis(500)).await;
            }

            // TODO: Check that channel is in the guild and that the bot has access to it before joining.

            if let Err(e) = sbird.join(guild_id, channel_id).await {
                error!("[{guild_id}] Failed to join voice channel: {e:?}");

                // Although we failed to join, we need to clear out existing event handlers on the call.
                _ = sbird.remove(guild_id).await;
                self.voice_handlers.remove(&guild_id);

                Err(format!("Failed to join voice channel: {e}"))
            } else {
                info!("[{guild_id}] Joined channel {channel_id}");
                Ok(())
            }
        } else {
            error!("[{guild_id}] Tried rejoin on {channel_id} but not currently in a call!");
            Err("Not currently recording a call!".to_string())
        }
    }

    pub async fn finish(&self, ctx: &Context, guild_id: GuildId) -> Result<RecordingSummary, String> {
        let sbird = songbird::get(ctx).await.expect("Songbird doesn't exist!");

        let has_call = sbird.get(guild_id).is_some();

        if has_call {
            let channel_id = {
                let call = sbird.get(guild_id).unwrap();
                ChannelId::from(call.lock().await.current_channel().unwrap().0)
            };

            if let Err(e) = sbird.remove(guild_id).await {
                error!("[{guild_id}] Failed to leave channel: {e:?}");
                Err(format!("Failed to leave channel: {e}"))
            } else {
                match self.voice_handlers.remove(&guild_id) {
                    None => {
                        error!("[{guild_id}] Error retrieving recording state!");
                        Err("Error retrieving recording state!".to_string())
                    }
                    Some((_, voice_handler)) => {
                        // TODO: Finalize recording in VoiceReceiver.
                        info!("[{guild_id}] Left channel {channel_id} and finalized recording!");

                        Ok(voice_handler.get_summary())
                    }
                }
            }
        } else {
            Err("Not in a voice channel!".to_string())
        }
    }
}