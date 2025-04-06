use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serenity::{
    async_trait,
    client::{Context, EventHandler},
    model::gateway::Ready,
};
use serenity::all::{Command, GuildId, Interaction, UserId};
use serenity::prelude::TypeMapKey;
use crate::commands;

#[derive(Clone)]
pub struct RecordingMetadata {
    pub started: DateTime<Utc>,
    pub guild_id: GuildId,
    pub output_dir: PathBuf,
    pub output_dir_name: String,
    pub known_users: Arc<RwLock<HashSet<UserId>>>,
}

impl TypeMapKey for RecordingMetadata {
    type Value = Arc<DashMap<GuildId, RecordingMetadata>>;
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