use std::sync::Arc;
use dashmap::DashMap;
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::gateway::Ready,
};
use serenity::all::{Command, GuildId, Interaction};
use serenity::prelude::TypeMapKey;
use tokio::sync::mpsc::Sender;
use crate::commands;
use crate::voice_handler::VoiceCommand;

pub struct DiscordData {
    pub(crate) voice_commands: DashMap<GuildId, Sender<VoiceCommand>>
}

impl TypeMapKey for DiscordData {
    type Value = Arc<DiscordData>;
}

pub struct Events;

#[async_trait]
impl EventHandler for Events {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        Command::set_global_commands(&ctx.http,
                                     vec![
                                         commands::voice::join::register(),
                                         commands::voice::leave::register(),
                                         commands::voice::recording::record::register(),
                                         commands::voice::recording::finish::register(),
                                     ]
        ).await.expect("Failed to register global commands!");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            match command.data.name.as_str() {
                commands::voice::join::NAME => commands::voice::join::run(&ctx, &command).await,
                commands::voice::leave::NAME => commands::voice::leave::run(&ctx, &command).await,
                commands::voice::recording::record::NAME => commands::voice::recording::record::run(&ctx, &command).await,
                commands::voice::recording::finish::NAME => commands::voice::recording::finish::run(&ctx, &command).await,
                _ => {}
            }
        }
    }
}