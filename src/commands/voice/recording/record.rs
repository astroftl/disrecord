use serenity::all::{ActivityData, ChannelType, CommandInteraction, CommandOptionType, Context, CreateCommandOption, CreateInteractionResponseMessage, GuildId, InteractionContext, OnlineStatus};
use serenity::builder::{CreateCommand, CreateInteractionResponse};
use tokio::sync::mpsc::Sender;
use crate::commands::voice::join::{do_join, get_member_channel};
use crate::discord::DiscordData;
use crate::voice_handler::VoiceCommand;

pub const NAME: &str = "record";

async fn do_record(ctx: &Context, guild_id: GuildId, cmd_tx: Sender<VoiceCommand>) -> Result<(), String> {
    match cmd_tx.send(VoiceCommand::Record).await {
        Ok(_) => {
            let activity = ActivityData::custom("Recording...");
            let status = OnlineStatus::DoNotDisturb;
            ctx.set_presence(Some(activity), status);

            let bot_name = ctx.cache.current_user().display_name().to_string();

            guild_id.edit_nickname(ctx, Some(format!("🔴 {bot_name}").as_str())).await.unwrap_or_else(|e| {
                warn!("Failed to set nickname: {e:?}");
            });
            
            Ok(())
        }
        Err(e) => {
            Err(format!("Error sending voice command: {e:?}"))
        }
    }
}

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
            match do_record(ctx, guild_id, cmd_tx.clone()).await {
                Ok(_) => {
                    let resp = CreateInteractionResponseMessage::new()
                        .content("Recording begun!")
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
                .content("Failed to get VoiceCommand Sender!")
                .ephemeral(true);

            cmd.create_response(ctx, CreateInteractionResponse::Message(resp)).await.unwrap_or_else(|e| {
                error!("Error responding to the interaction: {e:?}");
            });
        }
    } else {
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
            match do_join(ctx, guild_id, channel_id).await {
                Ok(cmd_tx) => {
                    match do_record(ctx, guild_id, cmd_tx).await {
                        Ok(_) => {
                            let resp = CreateInteractionResponseMessage::new()
                                .content(format!("Joined <#{channel_id}> and began recording!"))
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
                }
                Err(e) => {
                    let resp = CreateInteractionResponseMessage::new()
                        .content(format!("{e:?}"))
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
}

pub fn register() -> CreateCommand {
    CreateCommand::new(NAME)
        .description("Being recording current voice channel")
        .add_context(InteractionContext::Guild)
        .add_option(
            CreateCommandOption::new(CommandOptionType::Channel, "channel", "Voice channel to record")
                .channel_types(vec![ChannelType::Voice])
        )
}