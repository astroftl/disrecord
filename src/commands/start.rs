use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use chrono::Utc;
use serenity::all::{ActivityData, ChannelId, ChannelType, CommandInteraction, CommandOptionType, Context, CreateCommandOption, CreateInteractionResponseMessage, GuildId, InteractionContext, OnlineStatus, UserId};
use serenity::builder::{CreateCommand, CreateInteractionResponse};
use songbird::CoreEvent;
use crate::discord::RecordingMetadata;
use crate::voice_handler::VoiceReceiver;

pub const NAME: &str = "start";

pub async fn do_join(ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> Result<(), String> {
    trace!("Joining: {channel_id} @ {guild_id}");

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    // Some events relating to voice receive fire *while joining*.
    // We must make sure that any event handlers are installed before we attempt to join.
    if manager.get(guild_id).is_none() {
        let started = Utc::now();
        let base_dir= PathBuf::from("recordings");
        let output_dir_name = started.format("%Y_%m_%d_%H_%M_%S").to_string();
        let output_dir = base_dir.join(format!("{}", guild_id)).join(output_dir_name.as_str());

        let metadata_entry = RecordingMetadata {
            started,
            guild_id,
            output_dir: output_dir.clone(),
            output_dir_name,
            known_users: Arc::new(RwLock::new(HashSet::new())),
        };

        {
            let data = ctx.data.read().await;
            let metadata = data.get::<RecordingMetadata>().unwrap();
            metadata.insert(guild_id, metadata_entry.clone());
        }

        let call_lock = manager.get_or_insert(guild_id);
        let mut call = call_lock.lock().await;

        let evt_receiver = VoiceReceiver::new(metadata_entry).await;

        call.add_global_event(CoreEvent::SpeakingStateUpdate.into(), evt_receiver.clone());
        call.add_global_event(CoreEvent::ClientDisconnect.into(), evt_receiver.clone());
        call.add_global_event(CoreEvent::VoiceTick.into(), evt_receiver);
    }

    if let Err(e) = manager.join(guild_id, channel_id).await {
        error!("Failed to join voice channel: {e:?}");

        // Although we failed to join, we need to clear out existing event handlers on the call.
        _ = manager.remove(guild_id).await;

        Err(format!("Failed to join voice channel: {e}"))
    } else {
        let activity = ActivityData::custom("Recording...");
        let status = OnlineStatus::DoNotDisturb;
        ctx.set_presence(Some(activity), status);

        let bot_name = ctx.cache.current_user().display_name().to_string();

        guild_id.edit_nickname(ctx, Some(format!("🔴 {bot_name}").as_str())).await.unwrap_or_else(|e| {
            warn!("Failed to set nickname: {e:?}");
        });

        info!("[{guild_id}] Joined channel {channel_id} and began recording!");
        Ok(())
    }
}

pub async fn get_member_channel(ctx: &Context, guild_id: GuildId, user_id: UserId) -> Option<ChannelId> {
    let guild_channels = match guild_id.channels(ctx).await {
        Ok(x) => x,
        Err(e) => {
            error!("Failed to retrieve guild channels: {e:?}");
            return None;
        }
    };

    let voice_channels = guild_channels.iter().filter(|x| x.1.kind == ChannelType::Voice ).collect::<Vec<_>>();

    for channel in voice_channels {
        let members = match channel.1.members(ctx) {
            Ok(x) => x,
            Err(e) => {
                error!("Failed to retrieve members from channel {}: {e:?}", channel.0);
                return None;
            }
        };

        if let Some(_) = members.iter().find(|f| f.user.id == user_id) {
            return Some(*channel.0);
        }
    }

    None
}

async fn handle_join_and_record_with_response(ctx: &Context, cmd: &CommandInteraction, guild_id: GuildId, channel_id: ChannelId) {
    match do_join(ctx, guild_id, channel_id).await {
        Ok(_) => {
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("🔴 Joined <#{channel_id}> and began recording!"));

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
        Err(e) => {
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("{e}"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
    }
}

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let channel_opt = cmd.data.options.first();
    let channel_id = match channel_opt {
        Some(x) => {
            Some(x.value.as_channel_id().unwrap())
        }
        None => {
            get_member_channel(ctx, guild_id, cmd.user.id).await
        }
    };
    
    // TODO: Check that channel is in the guild and that the bot has access to it before joining.

    let has_call = manager.get(guild_id).is_some();

    if has_call {
        let resp = CreateInteractionResponseMessage::new()
            .content("Already recording! (use /rejoin to switch channels)")
            .ephemeral(true);

        cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
            error!("Error responding to the interaction: {e:?}");
        });
    } else {
        if let Some(channel_id) = channel_id {
            handle_join_and_record_with_response(ctx, cmd, guild_id, channel_id).await;
        } else {
            let resp = CreateInteractionResponseMessage::new()
                .content("You are not in a voice channel, and did not provide one as an argument!")
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
    }
}

pub fn register() -> CreateCommand {
    CreateCommand::new(NAME)
        .description("Join a voice channel and begin recording")
        .add_context(InteractionContext::Guild)
        .add_option(
            CreateCommandOption::new(CommandOptionType::Channel, "channel", "Voice channel to record")
                .channel_types(vec![ChannelType::Voice])
        )
}