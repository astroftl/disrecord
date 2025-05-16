use crate::recorder::voice_receiver::VoiceReceiver;
use crate::recorder::{RecorderConfig, RecordingSummary};
use serenity::all::{ChannelId, Context, CreateAttachment, CreateEmbed, CreateEmbedFooter, CreateMessage, GuildId, Message, MessageReference};
use songbird::CoreEvent;
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::time::sleep;
use crate::recorder::writer::{VoiceUpdate, Writer};

#[derive(Debug)]
pub struct Recorder {
    writer: Arc<Writer>,
    voice_tx: mpsc::Sender<VoiceUpdate>,
    resp_messages: DashMap<GuildId, (ChannelId, Option<Message>)>,
}

impl Recorder {
    pub fn new(config: RecorderConfig) -> Self {
        let (voice_tx, voice_rx) = mpsc::channel(1024);

        let writer = Arc::new(Writer::new(config));
        Writer::run(writer.clone(), voice_rx);

        Self {
            writer,
            voice_tx,
            resp_messages: DashMap::new(),
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
            let call_lock = sbird.get_or_insert(guild_id);
            let mut call = call_lock.lock().await;

            let voice_receiver = VoiceReceiver::new(guild_id, ctx, call_lock.clone(), self.voice_tx.clone()).await;
            self.writer.start(guild_id);

            call.add_global_event(CoreEvent::VoiceTick.into(), voice_receiver.clone());
            call.add_global_event(CoreEvent::ClientDisconnect.into(), voice_receiver.clone());
            call.add_global_event(CoreEvent::SpeakingStateUpdate.into(), voice_receiver);
        }

        // TODO: Check that channel is in the guild and that the bot has access to it before joining.

        if let Err(e) = sbird.join(guild_id, channel_id).await {
            error!("[{guild_id}] Failed to join voice channel: {e:?}");

            // Although we failed to join, we need to clear out existing event handlers on the call.
            _ = sbird.remove(guild_id).await;
            self.writer.finish(guild_id).await;

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

                    return Err(format!("Failed to leave voice channel: {e}"))
                };

                sleep(Duration::from_millis(500)).await;
            }

            // TODO: Check that channel is in the guild and that the bot has access to it before joining.

            if let Err(e) = sbird.join(guild_id, channel_id).await {
                error!("[{guild_id}] Failed to join voice channel: {e:?}");

                // Although we failed to join, we need to clear out existing event handlers on the call.
                _ = sbird.remove(guild_id).await;

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
                info!("[{guild_id}] Left channel {channel_id} and finalized recording!");

                match self.writer.finish(guild_id).await {
                    None => {
                        error!("[{guild_id}] Failed to finish recording!");
                        Err("Failed to finish recording".to_string())
                    },
                    Some(summary) => Ok(summary)
                }
            }
        } else {
            error!("[{guild_id}] Tried to finish but not in a voice channel!");
            Err("Not in a voice channel!".to_string())
        }
    }

    pub async fn finish_self(&self, ctx: &Context, guild_id: GuildId) {
        if let Ok(metadata) = self.finish(ctx, guild_id).await {
            if let Some((_, (cmd_channel, resp_msg))) = self.resp_messages.remove(&guild_id) {
                let duration = metadata.ended.signed_duration_since(metadata.started);

                let mut user_string = String::new();
                {
                    let known_users = metadata.known_users;

                    for known_user in known_users.iter() {
                        user_string += format!("<@{}> ", known_user.get()).as_str()
                    }
                }
                user_string.pop();

                let hours = duration.num_hours();
                let minutes = duration.num_minutes() - (duration.num_hours() * 60);
                let seconds  = duration.num_seconds() - (duration.num_minutes() * 60);

                let mut resp = CreateMessage::new()
                    .embed(CreateEmbed::new()
                        .title("Recording finished!")
                        .field("Duration", format!("{hours}h {minutes:02}m {seconds:02}s"), false)
                        .field("Users Recorded", user_string, false)
                        .footer(CreateEmbedFooter::new("For recording started"))
                        .timestamp(metadata.started)
                    );

                if let Some(resp_msg) = resp_msg {
                    let mut msg_ref = MessageReference::from(&resp_msg);
                    msg_ref.fail_if_not_exists = Some(false);
                    resp = resp.reference_message(msg_ref);
                }

                let posted_message = match cmd_channel.send_message(ctx, resp).await {
                    Ok(x) => Some(x),
                    Err(e) => {
                        error!("Error editing response to the interaction: {e:?}");
                        None
                    }
                };

                match metadata.zip_rx.await {
                    Ok(x) => {
                        match x {
                            Ok(zip_path) => {
                                let fup_attachment = match CreateAttachment::path(zip_path).await {
                                    Ok(x) => x,
                                    Err(e) => {
                                        error!("Failed to create attachment: {e:?}");
                                        return;
                                    }
                                };

                                let mut followup = CreateMessage::new().add_file(fup_attachment);

                                if let Some(posted_message) = &posted_message {
                                    let mut msg_ref = MessageReference::from(posted_message);
                                    msg_ref.fail_if_not_exists = Some(false);
                                    followup = followup.reference_message(msg_ref);
                                }

                                if let Err(e) = cmd_channel.send_message(ctx, followup).await {
                                    error!("Error sending followup to the interaction: {e:?}");
                                    let mut followup = CreateMessage::new().content(format!("Failed to send .zip (file too large?): {e:?}"));

                                    if let Some(posted_message) = &posted_message {
                                        let mut msg_ref = MessageReference::from(posted_message);
                                        msg_ref.fail_if_not_exists = Some(false);
                                        followup = followup.reference_message(msg_ref);
                                    }

                                    if let Err(e) = cmd_channel.send_message(ctx, followup).await {
                                        error!("Error sending followup to explain why the followup failed (ironic): {e:?}");
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to zip recordings: {e:?}");
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to receive zipper message: {e:?}");
                    }
                }

                match metadata.mix_rx.await {
                    Ok(x) => {
                        match x {
                            Ok(mix_path) => {
                                let fup_attachment = match CreateAttachment::path(mix_path).await {
                                    Ok(x) => x,
                                    Err(e) => {
                                        error!("Failed to create attachment: {e:?}");
                                        return;
                                    }
                                };

                                let mut followup = CreateMessage::new().add_file(fup_attachment);

                                if let Some(posted_message) = &posted_message {
                                    let mut msg_ref = MessageReference::from(posted_message);
                                    msg_ref.fail_if_not_exists = Some(false);
                                    followup = followup.reference_message(msg_ref);
                                }

                                if let Err(e) = cmd_channel.send_message(ctx, followup).await {
                                    error!("Error sending followup to the interaction: {e:?}");
                                    let mut followup = CreateMessage::new().content(format!("Failed to send mixed .opus (file too large?): {e:?}"));

                                    if let Some(posted_message) = &posted_message {
                                        let mut msg_ref = MessageReference::from(posted_message);
                                        msg_ref.fail_if_not_exists = Some(false);
                                        followup = followup.reference_message(msg_ref);
                                    }

                                    if let Err(e) = cmd_channel.send_message(ctx, followup).await {
                                        error!("Error sending followup to explain why the followup failed (ironic): {e:?}");
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to mix recordings: {e:?}");
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to receive mixer message: {e:?}");
                    }
                }
            } else {
                error!("[{guild_id}] Failed to get response metadata to send summary message!");
            }
        }
    }

    pub fn set_resp(&self, guild_id: GuildId, resp: (ChannelId, Option<Message>)) {
        self.resp_messages.insert(guild_id, resp);
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        trace!("Recorder::drop");
    }
}