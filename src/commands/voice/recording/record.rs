use serenity::all::{ActivityData, CommandInteraction, Context, CreateInteractionResponseMessage, InteractionContext, OnlineStatus};
use serenity::builder::{CreateCommand, CreateInteractionResponse};
use crate::discord::DiscordData;
use crate::voice_handler::VoiceCommand;

pub const NAME: &str = "record";
pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    let has_call = manager.get(guild_id).is_some();

    if has_call {
        let data = ctx.data.read().await.get::<DiscordData>().unwrap().clone();
        if let Some(cmd_tx) = data.voice_commands.get(&guild_id) {
            match cmd_tx.send(VoiceCommand::Record).await {
                Ok(_) => {
                    let activity = ActivityData::custom("Recording...");
                    let status = OnlineStatus::DoNotDisturb;
                    ctx.set_presence(Some(activity), status);

                    let bot_name = ctx.cache.current_user().display_name().to_string();

                    guild_id.edit_nickname(ctx, Some(format!("🔴 {bot_name}").as_str())).await.unwrap_or_else(|e| {
                        warn!("Failed to set nickname: {e:?}");
                    });

                    let resp = CreateInteractionResponseMessage::new()
                        .content("Recording begun!")
                        .ephemeral(true);

                    cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                        error!("Error responding to the interaction: {e:?}");
                    });
                }
                Err(e) => {
                    let resp = CreateInteractionResponseMessage::new()
                        .content(format!("Error sending voice command: {e:?}"))
                        .ephemeral(true);

                    cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                        error!("Error responding to the interaction: {e:?}");
                    });
                }
            }
        } else {
            let resp = CreateInteractionResponseMessage::new()
                .content("Failed to get VoiceCommand!")
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
        .description("Being recording current voice channel")
        .add_context(InteractionContext::Guild)
}