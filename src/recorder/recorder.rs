use crate::config::AppConfig;
use crate::recorder::RecordingSummary;
use crate::recorder::voice_receiver::VoiceReceiver;
use crate::recorder::writer::{VoiceUpdate, Writer};
use dashmap::DashMap;
use serenity::all::{ChannelId, CommandInteraction, Context, CreateAttachment, CreateEmbed, CreateEmbedFooter, CreateInteractionResponse, CreateInteractionResponseMessage, CreateMessage, EditInteractionResponse, GuildId, Message, MessageReference};
use songbird::CoreEvent;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[derive(Debug)]
pub struct Recorder {
    writer: Arc<Writer>,
    config: Arc<AppConfig>,
    voice_tx: mpsc::Sender<VoiceUpdate>,
    resp_messages: DashMap<GuildId, (ChannelId, Option<Message>)>,
}

enum FinishType<'a> {
    Command(&'a CommandInteraction),
    Message(FinishResponse),
}

struct FinishResponse {
    channel: ChannelId,
    message: Option<Message>,
}

impl Recorder {
    pub fn new(config: Arc<AppConfig>) -> Self {
        let (voice_tx, voice_rx) = mpsc::channel(1024);

        let writer = Arc::new(Writer::new(config.clone()));
        Writer::run(writer.clone(), voice_rx);

        Self {
            writer,
            config,
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

        // TODO: Clean up by getting call once, then checking Option and retrieving current_channel
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

    // TODO: Move this stuff to commands? Or somewhere else; formatting isn't really Recorder stuff
    // Also, that would clean up duplicate code from /finish
    pub async fn handle_finish(&self, ctx: &Context, guild_id: GuildId, cmd: Option<&CommandInteraction>) {
        match self.finish(ctx, guild_id).await {
            Ok(metadata) => {
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

                let finish_type = if let Some(cmd) = cmd {
                    FinishType::Command(cmd)
                } else {
                    match self.resp_messages.remove(&guild_id) {
                        Some((_, (channel, message))) => FinishType::Message(FinishResponse{ channel, message }),
                        None => {
                            error!("[{guild_id}] Failed to get response metadata to send summary message!");
                            return
                        }
                    }
                };

                let report_embed = CreateEmbed::new()
                    .title("Recording finished!")
                    .field("Duration", format!("{hours}h {minutes:02}m {seconds:02}s"), false)
                    .field("Users Recorded", user_string, false)
                    .footer(CreateEmbedFooter::new("For recording started"))
                    .timestamp(metadata.started);

                let report_message = match finish_type {
                    FinishType::Command(cmd) => {
                        let resp = EditInteractionResponse::new().embed(report_embed);
                        match cmd.edit_response(ctx, resp).await {
                            Ok(message) => FinishResponse {
                                channel: cmd.channel_id,
                                message: Some(message),
                            },
                            Err(e) => {
                                error!("Error editing response to the interaction: {e:?}");
                                FinishResponse {
                                    channel: cmd.channel_id,
                                    message: None,
                                }
                            }
                        }
                    }
                    FinishType::Message(FinishResponse{ channel, message }) => {
                        let mut resp = CreateMessage::new().embed(report_embed);

                        if let Some(resp_msg) = message {
                            let mut msg_ref = MessageReference::from(&resp_msg);
                            msg_ref.fail_if_not_exists = Some(false);
                            resp = resp.reference_message(msg_ref);
                        }

                        match channel.send_message(ctx, resp).await {
                            Ok(x) => FinishResponse {
                                channel,
                                message: Some(x),
                            },
                            Err(e) => {
                                error!("Error editing response to the interaction: {e:?}");
                                FinishResponse {
                                    channel,
                                    message: None,
                                }
                            }
                        }
                    }
                };

                match metadata.zip_rx.await {
                    Ok(x) => {
                        match x {
                            Ok(zip_path) => {
                                if self.config.get_output(&guild_id).embed_files.unwrap_or(false) {
                                    let fup_attachment = match CreateAttachment::path(zip_path.clone()).await {
                                        Ok(x) => x,
                                        Err(e) => {
                                            error!("Failed to create attachment: {e:?}");
                                            return;
                                        }
                                    };

                                    let mut followup = CreateMessage::new().add_file(fup_attachment);

                                    if let Some(posted_message) = &report_message.message {
                                        let mut msg_ref = MessageReference::from(posted_message);
                                        msg_ref.fail_if_not_exists = Some(false);
                                        followup = followup.reference_message(msg_ref);
                                    }

                                    if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                        error!("Error sending followup to the interaction: {e:?}");
                                        let mut followup = CreateMessage::new().content("Failed to send .zip (file too large?)");

                                        if let Some(posted_message) = &report_message.message {
                                            let mut msg_ref = MessageReference::from(posted_message);
                                            msg_ref.fail_if_not_exists = Some(false);
                                            followup = followup.reference_message(msg_ref);
                                        }

                                        if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                            error!("Error sending followup to explain why the followup failed (ironic): {e:?}");
                                        }
                                    }
                                }

                                if let Some(post_cmd) = &self.config.get_output(&guild_id).post_command {
                                    let format_vars: HashMap<String, String> = HashMap::from([
                                        ("filename".to_string(), zip_path.to_string_lossy().to_string()),
                                        ("guild".to_string(), guild_id.to_string()),
                                        ("time".to_string(), metadata.started.format(self.config.date_format.as_str()).to_string()),
                                    ]);

                                    let post_cmd_formatted = strfmt::strfmt(&post_cmd, &format_vars).unwrap();
                                    if let Some(args) = shlex::split(post_cmd_formatted.as_str()) {
                                        debug!("[{guild_id}] Executing post command '{}' with args: {:?}", &args[0], &args[1..]);
                                        let output = Command::new(&args[0])
                                            .args(&args[1..])
                                            .output()
                                            .await;

                                        if let Ok(output) = output {
                                            if output.status.success() {
                                                let stdout = String::from_utf8_lossy(&output.stdout);
                                                debug!("Post Command Success: {}", stdout);
                                                let mut followup = CreateMessage::new().content(stdout);
                                                if let Some(posted_message) = &report_message.message {
                                                    let mut msg_ref = MessageReference::from(posted_message);
                                                    msg_ref.fail_if_not_exists = Some(false);
                                                    followup = followup.reference_message(msg_ref);
                                                }
                                                if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                                    error!("Error sending followup to the interaction: {e:?}");
                                                }
                                            } else {
                                                let stderr = String::from_utf8_lossy(&output.stderr);
                                                error!("Post Command Error: {}", stderr);
                                                let mut followup = CreateMessage::new().content(stderr);
                                                if let Some(posted_message) = &report_message.message {
                                                    let mut msg_ref = MessageReference::from(posted_message);
                                                    msg_ref.fail_if_not_exists = Some(false);
                                                    followup = followup.reference_message(msg_ref);
                                                }
                                                if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                                    error!("Error sending followup to the interaction: {e:?}");
                                                }
                                            }
                                        }
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

                for mix_rx in metadata.mix_rxs {
                    match mix_rx.await {
                        Ok(x) => {
                            match x {
                                Ok(mix_path) => {
                                    if self.config.get_output(&guild_id).embed_files.unwrap_or(false) {
                                        let fup_attachment = match CreateAttachment::path(mix_path.clone()).await {
                                            Ok(x) => x,
                                            Err(e) => {
                                                error!("Failed to create attachment: {e:?}");
                                                return;
                                            }
                                        };

                                        let mut followup = CreateMessage::new().add_file(fup_attachment);

                                        if let Some(posted_message) = &report_message.message {
                                            let mut msg_ref = MessageReference::from(posted_message);
                                            msg_ref.fail_if_not_exists = Some(false);
                                            followup = followup.reference_message(msg_ref);
                                        }

                                        if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                            error!("Error sending followup to the interaction: {e:?}");
                                            let mut followup = CreateMessage::new().content("Failed to send mixed .opus (file too large?)");

                                            if let Some(posted_message) = &report_message.message {
                                                let mut msg_ref = MessageReference::from(posted_message);
                                                msg_ref.fail_if_not_exists = Some(false);
                                                followup = followup.reference_message(msg_ref);
                                            }

                                            if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                                error!("Error sending followup to explain why the followup failed (ironic): {e:?}");
                                            }
                                        }
                                    }

                                    if let Some(post_cmd) = &self.config.get_output(&guild_id).post_command {
                                        let format_vars: HashMap<String, String> = HashMap::from([
                                            ("filename".to_string(), mix_path.to_string_lossy().to_string()),
                                            ("guild".to_string(), guild_id.to_string()),
                                            ("time".to_string(), metadata.started.format(self.config.date_format.as_str()).to_string()),
                                        ]);

                                        let post_cmd_formatted = strfmt::strfmt(&post_cmd, &format_vars).unwrap();
                                        if let Some(args) = shlex::split(post_cmd_formatted.as_str()) {
                                            debug!("[{guild_id}] Executing post command '{}' with args: {:?}", &args[0], &args[1..]);
                                            let output = Command::new(&args[0])
                                                .args(&args[1..])
                                                .output()
                                                .await;

                                            if let Ok(output) = output {
                                                if output.status.success() {
                                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                                    debug!("Post Command Success: {}", stdout);
                                                    let mut followup = CreateMessage::new().content(stdout);
                                                    if let Some(posted_message) = &report_message.message {
                                                        let mut msg_ref = MessageReference::from(posted_message);
                                                        msg_ref.fail_if_not_exists = Some(false);
                                                        followup = followup.reference_message(msg_ref);
                                                    }
                                                    if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                                        error!("Error sending followup to the interaction: {e:?}");
                                                    }
                                                } else {
                                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                                    error!("Post Command Error: {}", stderr);
                                                    let mut followup = CreateMessage::new().content(stderr);
                                                    if let Some(posted_message) = &report_message.message {
                                                        let mut msg_ref = MessageReference::from(posted_message);
                                                        msg_ref.fail_if_not_exists = Some(false);
                                                        followup = followup.reference_message(msg_ref);
                                                    }
                                                    if let Err(e) = report_message.channel.send_message(ctx, followup).await {
                                                        error!("Error sending followup to the interaction: {e:?}");
                                                    }
                                                }
                                            }
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
                }
            }
            Err(e) => {
                if let Some(cmd) = cmd {
                    let resp = CreateInteractionResponseMessage::new()
                        .content(format!("Failed to finish recording: {e}"))
                        .ephemeral(true);

                    cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                        error!("Error responding to the interaction: {e:?}");
                    });
                }
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