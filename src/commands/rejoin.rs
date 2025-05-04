use std::time::Duration;
use serenity::all::{ChannelId, ChannelType, CommandInteraction, CommandOptionType, Context, CreateCommandOption, GuildId, InteractionContext};
use serenity::builder::{CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage};
use tokio::time::sleep;
use crate::commands::finish::reset_presence;
use crate::commands::start::get_member_channel;

pub const NAME: &str = "rejoin";

pub async fn do_rejoin(ctx: &Context, guild_id: GuildId, channel_id: ChannelId) -> Result<(), String> {
    trace!("Re-joining: {channel_id} @ {guild_id}");

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialization.")
        .clone();

    if manager.get(guild_id).is_some() {
        let old_channel_id = {
            let call = manager.get(guild_id).unwrap();
            ChannelId::from(call.lock().await.current_channel().unwrap().0)
        };

        if old_channel_id == channel_id {
            if let Err(e) = manager.leave(guild_id).await {
                error!("Failed to leave voice channel: {e:?}");

                // Although we failed to join, we need to clear out existing event handlers on the call.
                _ = manager.remove(guild_id).await;

                reset_presence(ctx, guild_id).await;

                return Err(format!("Failed to leave voice channel: {e}"))
            };

            sleep(Duration::from_millis(500)).await;
        }

        // TODO: Check that channel is in the guild and that the bot has access to it before joining.

        if let Err(e) = manager.join(guild_id, channel_id).await {
            error!("Failed to join voice channel: {e:?}");

            // Although we failed to join, we need to clear out existing event handlers on the call.
            _ = manager.remove(guild_id).await;

            reset_presence(ctx, guild_id).await;

            Err(format!("Failed to join voice channel: {e}"))
        } else {
            info!("Joined channel {channel_id} of guild {guild_id}!");
            Ok(())
        }
    } else {
        error!("[{guild_id}] tried rejoin on {channel_id} but not currently in a call!");
        Err("Not currently recording a call!".to_string())
    }
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
        match do_rejoin(ctx, guild_id, channel_id).await {
            Ok(_) => {
                let resp = CreateInteractionResponseMessage::new()
                    .content(format!("Joined <#{channel_id}>!"))
                    .ephemeral(true);

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
        .description("Leave and rejoin a voice channel")
        .add_context(InteractionContext::Guild)
        .add_option(
            CreateCommandOption::new(CommandOptionType::Channel, "channel", "Voice channel to join")
                .channel_types(vec![ChannelType::Voice])
        )
}