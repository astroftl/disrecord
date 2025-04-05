use serenity::all::{ChannelId, ChannelType, CommandInteraction, CommandOptionType, Context, CreateCommandOption, GuildId, InteractionContext, UserId};
use serenity::builder::{CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage};
use songbird::CoreEvent;
use tokio::sync::mpsc::{channel, Sender};
use crate::discord::DiscordData;
use crate::voice_handler::{VoiceCommand, VoiceReceiver};

pub const NAME: &str = "join";

pub async fn do_join(ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> Result<Sender<VoiceCommand>, ()> {
    debug!("Joining: {channel_id:?} @ {guild_id:?}");

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    // Some events relating to voice receive fire *while joining*.
    // We must make sure that any event handlers are installed before we attempt to join.
    let cmd_tx = if manager.get(guild_id).is_none() {
        let call_lock = manager.get_or_insert(guild_id);
        let mut call = call_lock.lock().await;

        let (cmd_tx, cmd_rx) = channel(32);
        let data = ctx.data.read().await.get::<DiscordData>().unwrap().clone();
        data.voice_commands.insert(guild_id, cmd_tx.clone());
        
        let evt_receiver = VoiceReceiver::new(guild_id, cmd_rx).await;

        call.add_global_event(CoreEvent::SpeakingStateUpdate.into(), evt_receiver.clone());
        call.add_global_event(CoreEvent::ClientDisconnect.into(), evt_receiver.clone());
        call.add_global_event(CoreEvent::VoiceTick.into(), evt_receiver);

        cmd_tx
    } else {
        let data = ctx.data.read().await.get::<DiscordData>().unwrap().clone();
        if let Some(cmd_tx) = data.voice_commands.get(&guild_id) {
            cmd_tx.clone()
        } else {
            error!("Failed to get command sender for existing call handler!");
            return Err(());
        }
    };

    if let Ok(_) = manager.join(guild_id, channel_id).await {
        Ok(cmd_tx)
    } else {
        // Although we failed to join, we need to clear out existing event handlers on the call.
        _ = manager.remove(guild_id).await;
        
        Err(())
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

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();
    let channel_opt = cmd.data.options.first();
    let channel_id = match channel_opt {
        Some(x) => {
            Some(x.value.as_channel_id().unwrap())
        }
        None => {
            get_member_channel(ctx, guild_id, cmd.user.id).await
        }
    };

    if let Some(channel_id) = channel_id {
        if let Ok(_) = do_join(ctx, guild_id, channel_id).await {
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("Joined <#{channel_id}>"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        } else {
            let resp = CreateInteractionResponseMessage::new()
                .content("Failed to join channel!")
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
    } else {
        let resp = CreateInteractionResponseMessage::new()
            .content("You are not in a voice channel, and did not provide one as an argument!")
            .ephemeral(true);

        cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
            error!("Error responding to the interaction: {e:?}");
        });
    }
}

pub fn register() -> CreateCommand {
    CreateCommand::new(NAME)
        .description("Join a voice channel")
        .add_context(InteractionContext::Guild)
        .add_option(
            CreateCommandOption::new(CommandOptionType::Channel, "channel", "Voice channel to join")
                .channel_types(vec![ChannelType::Voice])
        )
}