use serenity::all::{ChannelId, CommandInteraction, Context, CreateInteractionResponseMessage, InteractionContext};
use serenity::builder::{CreateCommand, CreateInteractionResponse};
use crate::discord::DiscordData;

pub const NAME: &str = "leave";

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();
    
    let data = ctx.data.read().await.get::<DiscordData>().unwrap().clone();
    data.voice_commands.remove(&guild_id);

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
                .content(format!("Failed: {e:?}"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        } else {
            let resp = CreateInteractionResponseMessage::new()
                .content(format!("Left <#{channel_id}>"))
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
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
        .description("Leave a voice channel")
        .add_context(InteractionContext::Guild)
}