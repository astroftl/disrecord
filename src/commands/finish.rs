use std::fs::{read_dir, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use serenity::all::{ChannelId, CommandInteraction, Context, CreateAttachment, CreateEmbed, CreateEmbedFooter, CreateInteractionResponseFollowup, CreateInteractionResponseMessage, EditInteractionResponse, GuildId, InteractionContext, OnlineStatus};
use serenity::builder::{CreateCommand, CreateInteractionResponse};
use chrono::Utc;
use tokio::task;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;
use crate::discord::RecordingMetadata;

pub const NAME: &str = "finish";

pub async fn reset_presence(ctx: &Context, guild_id: GuildId) {
    let status = OnlineStatus::Online;
    ctx.set_presence(None, status);

    guild_id.edit_nickname(ctx, None).await.unwrap_or_else(|e| {
        warn!("Failed to set nickname: {e}");
    });
}

fn zip_output_files(metadata: &RecordingMetadata) -> Result<PathBuf, String> {
    let zip_path = metadata.output_dir.join(format!("{}.zip", metadata.output_dir_name));
    debug!("[{}] Creating zip archive at {}", metadata.guild_id, zip_path.display());

    let zip_file = match File::create(&zip_path) {
        Ok(x) => x,
        Err(e) => {
            error!("[{}] Failed to create zip file {}: {e:?}", metadata.guild_id, zip_path.display());
            return Err(format!("Failed to create zip file: {e}"));
        }
    };

    let mut zip = ZipWriter::new(zip_file);

    let options =  SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let dir_entries = match read_dir(&metadata.output_dir) {
        Ok(x) => x,
        Err(e) => {
            error!("[{}] Failed to read recording directory {}: {e:?}", metadata.guild_id, metadata.output_dir.display());
            return Err(format!("Failed to create zip archive: {e}"));
        }
    };

    let mut buffer = Vec::new();

    for entry in dir_entries {
        let entry = match entry {
            Ok(x) => x,
            Err(e) => {
                error!("[{}] Failed to get directory entry: {e:?}", metadata.guild_id);
                return Err(format!("Failed to get directory entry: {e}"));
            }
        };

        let path = entry.path();

        if path == zip_path {
            continue;
        }

        let file_name = match path.file_name() {
            Some(x) => x,
            None => {
                error!("[{}] Failed to get file name! (this should never happen)", metadata.guild_id);
                return Err("Failed to get file name! (this should never happen)".to_string());
            }
        };

        let file_name = file_name.to_string_lossy().to_string();

        debug!("[{}] Adding {} to zip archive", metadata.guild_id, file_name);

        if let Err(e) = zip.start_file(file_name, options) {
            error!("[{}] Failed to start zip file: {e:?}", metadata.guild_id);
            return Err(format!("Failed to start zip file: {e}"));
        }

        let mut file = match File::open(&path) {
            Ok(x) => x,
            Err(e) => {
                error!("[{}] Failed to open file for reading: {e:?}", metadata.guild_id);
                return Err(format!("Failed to open file for reading: {e}"));
            }
        };

        if let Err(e) = file.read_to_end(&mut buffer) {
            error!("[{}] Failed to read file into buffer: {e:?}", metadata.guild_id);
            return Err(format!("Failed to read file into buffer: {e}"));
        }

        if let Err(e) = zip.write_all(&buffer) {
            error!("[{}] Failed to write buffer into zip: {e:?}", metadata.guild_id);
            return Err(format!("Failed to write buffer into zip: {e}"));
        }
    }

    if let Err(e) = zip.finish() {
        error!("[{}] Failed to finish zip file: {e:?}", metadata.guild_id);
        return Err(format!("Failed to finish zip file: {e}"));
    }

    info!("[{}] Created zip archive at {}", metadata.guild_id, zip_path.display());

    Ok(zip_path)
}

pub async fn do_finish(ctx: &Context, guild_id: GuildId) -> Result<RecordingMetadata, String> {
    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let has_call = manager.get(guild_id).is_some();

    if has_call {
        let channel_id = {
            let call = manager.get(guild_id).unwrap();
            ChannelId::from(call.lock().await.current_channel().unwrap().0)
        };

        if let Err(e) = manager.remove(guild_id).await {
            Err(format!("Failed to leave channel: {e}"))
        } else {
            reset_presence(ctx, guild_id).await;

            info!("Left channel {channel_id} of guild {guild_id} and finalized recording!");

            let data = ctx.data.read().await;
            let metadata = data.get::<RecordingMetadata>().unwrap();
            match metadata.remove(&guild_id) {
                Some(x) => {
                    Ok(x.1)
                }
                None => {
                    error!("[{guild_id}] Failed to retrieve recording metadata!");
                    Err("Failed to retrieve recording metadata!".to_string())
                }
            }
        }
    } else {
        Err("Not in a voice channel!".to_string())
    }
}

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();

    cmd.create_response(ctx, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new().content("Finishing recording..."))).await.unwrap_or_else(|e| {
        error!("Error responding to the interaction: {e:?}");
    });

    match do_finish(ctx, guild_id).await {
        Ok(recording_metadata) => {
            let duration = Utc::now().signed_duration_since(recording_metadata.started);

            let mut user_string = String::new();
            {
                let known_users = recording_metadata.known_users.read().unwrap();
                
                for known_user in known_users.iter() {
                    user_string += format!("<@{}> ", known_user.get()).as_str()
                }
            }
            user_string.pop();

            let resp = EditInteractionResponse::new()
                .embed(CreateEmbed::new()
                    .title("Recording finished!")
                    .field("Duration", format!("{}h {:02}m {:02}s", duration.num_hours(), duration.num_minutes(), duration.num_seconds()), false)
                    .field("Users Recorded", user_string, false)
                    .footer(CreateEmbedFooter::new("For recording started"))
                    .timestamp(recording_metadata.started)
                );

            if let Err(e) = cmd.edit_response(ctx, resp).await {
                error!("Error editing response to the interaction: {e:?}");
            }

            let zip_res = match task::spawn_blocking(move || {
                zip_output_files(&recording_metadata)
            }).await {
                Ok(x) => x,
                Err(e) => {
                    error!("Failed to join recording zip handle: {e:?}");
                    return;
                }
            };

            match zip_res {
                Ok(zip_path) => {
                    let fup_attachment = match CreateAttachment::path(zip_path).await {
                        Ok(x) => x,
                        Err(e) => {
                            error!("Failed to create attachment: {e:?}");
                            return;
                        }
                    };

                    let followup = CreateInteractionResponseFollowup::new().add_file(fup_attachment);

                    if let Err(e) = cmd.create_followup(ctx, followup).await {
                        error!("Error sending followup to the interaction: {e:?}");
                    }
                }
                Err(e) => {
                    error!("Failed to zip recordings: {e:?}");
                }
            }
        }
        Err(e) => {
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("Failed to finish recording: {e}"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
    }
}

pub fn register() -> CreateCommand {
    CreateCommand::new(NAME)
        .description("Finalize recording and leave voice channel")
        .add_context(InteractionContext::Guild)
}