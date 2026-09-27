use crate::recorder::recorder::Recorder;
use serenity::all::{CommandInteraction, Context, CreateInteractionResponseMessage, InteractionContext};
use serenity::builder::{CreateCommand, CreateInteractionResponse};

pub const NAME: &str = "finish";

pub async fn run(ctx: &Context, cmd: &CommandInteraction) {
    let guild_id = cmd.guild_id.unwrap();

    cmd.create_response(ctx, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new().content("Finishing recording..."))).await.unwrap_or_else(|e| {
        error!("Error responding to the interaction: {e:?}");
    });

    let rec_man = Recorder::get(ctx).await.expect("RecordManager doesn't exist!");

    rec_man.handle_finish(ctx, guild_id, Some(cmd)).await;
}

pub fn register() -> CreateCommand {
    CreateCommand::new(NAME)
        .description("Finalize recording and leave voice channel")
        .add_context(InteractionContext::Guild)
}