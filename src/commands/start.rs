use crate::commands::{get_channel_or_default_current, reset_presence, set_presence};
use crate::recorder::record_manager::RecordManager;
use serenity::all::{ChannelId, ChannelType, CommandInteraction, CommandOptionType, Context, CreateCommandOption, CreateInteractionResponseMessage, GuildId, InteractionContext};
use serenity::builder::{CreateCommand, CreateInteractionResponse};

pub const NAME: &str = "start";

async fn handle_join_and_record_with_response(ctx: &Context, cmd: &CommandInteraction, guild_id: GuildId, channel_id: ChannelId) {
    let rec_man = RecordManager::get(ctx).await.expect("RecordManager doesn't exist!");

    match rec_man.join(ctx, guild_id, channel_id).await {
        Ok(_) => {
            set_presence(ctx, guild_id).await;
            
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("🔴 Joined <#{channel_id}> and began recording!"));

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
        Err(e) => {
            reset_presence(ctx, guild_id).await;

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
    let channel_id = get_channel_or_default_current(ctx, cmd).await;
    
    // TODO: Check that channel is in the guild and that the bot has access to it before joining.

    let has_call = RecordManager::has_call(ctx, guild_id).await;

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