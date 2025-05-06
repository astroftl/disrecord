use crate::commands::get_channel_or_default_current;
use crate::recorder::recorder::Recorder;
use serenity::all::{ChannelType, CommandInteraction, CommandOptionType, Context, CreateCommandOption, InteractionContext};
use serenity::builder::{CreateCommand, CreateInteractionResponse, CreateInteractionResponseMessage};

pub const NAME: &str = "rejoin";

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();
    let channel_id = get_channel_or_default_current(ctx, cmd).await;

    if let Some(channel_id) = channel_id {
        let rec_man = Recorder::get(ctx).await.expect("RecordManager doesn't exist!");

        match rec_man.rejoin(ctx, guild_id, channel_id).await {
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