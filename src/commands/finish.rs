use crate::recorder::recorder::Recorder;
use serenity::all::{CommandInteraction, Context, CreateAttachment, CreateEmbed, CreateEmbedFooter, CreateInteractionResponseFollowup, CreateInteractionResponseMessage, EditInteractionResponse, InteractionContext};
use serenity::builder::{CreateCommand, CreateInteractionResponse};

pub const NAME: &str = "finish";

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();

    cmd.create_response(ctx, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new().content("Finishing recording..."))).await.unwrap_or_else(|e| {
        error!("Error responding to the interaction: {e:?}");
    });

    let rec_man = Recorder::get(ctx).await.expect("RecordManager doesn't exist!");

    match rec_man.finish(ctx, guild_id).await {
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

            let resp = EditInteractionResponse::new()
                .embed(CreateEmbed::new()
                    .title("Recording finished!")
                    .field("Duration", format!("{hours}h {minutes:02}m {seconds:02}s"), false)
                    .field("Users Recorded", user_string, false)
                    .footer(CreateEmbedFooter::new("For recording started"))
                    .timestamp(metadata.started)
                );

            if let Err(e) = cmd.edit_response(ctx, resp).await {
                error!("Error editing response to the interaction: {e:?}");
            }

            match metadata.zip_rx.await {
                Ok(x) => {
                    match x {
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
                                let followup = CreateInteractionResponseFollowup::new().content(format!("Failed to send .zip (file too large?): {e:?}"));
                                if let Err(e) = cmd.create_followup(ctx, followup).await {
                                    error!("Error sending followup to explain why the followup failed (ironic): {e:?}");
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