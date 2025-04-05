use serenity::all::{ChannelId, CommandInteraction, Context, CreateInteractionResponseMessage, GuildId, InteractionContext, OnlineStatus};
use serenity::builder::{CreateCommand, CreateInteractionResponse};

pub const NAME: &str = "finish";

pub async fn reset_presence(ctx: &Context, guild_id: GuildId) {
    let status = OnlineStatus::Online;
    ctx.set_presence(None, status);

    guild_id.edit_nickname(ctx, None).await.unwrap_or_else(|e| {
        warn!("Failed to set nickname: {e}");
    });
}

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();

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
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("Failed to leave channel: {e}"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        } else {
            reset_presence(ctx, guild_id).await;

            let resp = CreateInteractionResponseMessage::new()
                .content(format!("Left <#{channel_id}>"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });

            info!("Left channel {channel_id} of guild {guild_id}!");
        }
    } else {
        let resp = CreateInteractionResponseMessage::new()
            .content("Not in a voice channel!")
            .ephemeral(true);

        cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
            error!("Error responding to the interaction: {e:?}");
        });
    }
}

pub fn register() -> CreateCommand {
    CreateCommand::new(NAME)
        .description("Finalize recording and leave voice channel")
        .add_context(InteractionContext::Guild)
}