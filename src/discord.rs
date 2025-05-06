use crate::commands;
use crate::recorder::record_manager::RecordManager;
use serenity::all::{Command, Interaction};
use serenity::prelude::TypeMapKey;
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::gateway::Ready,
};
use std::sync::Arc;

impl TypeMapKey for RecordManager {
    type Value = Arc<RecordManager>;
}

pub struct Events;

#[async_trait]
impl EventHandler for Events {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        Command::set_global_commands(&ctx.http,
                                     vec![
                                         commands::start::register(),
                                         commands::finish::register(),
                                         commands::rejoin::register(),
                                     ]
        ).await.expect("Failed to register global commands!");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            match command.data.name.as_str() {
                commands::start::NAME => commands::start::run(&ctx, &command).await,
                commands::finish::NAME => commands::finish::run(&ctx, &command).await,
                commands::rejoin::NAME => commands::rejoin::run(&ctx, &command).await,
                _ => {}
            }
        }
    }
}