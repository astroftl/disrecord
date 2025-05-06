use serenity::all::{ActivityData, ChannelId, ChannelType, CommandInteraction, Context, GuildId, OnlineStatus, UserId};

pub mod start;
pub mod finish;
pub mod rejoin;

pub async fn set_presence(ctx: &Context, guild_id: GuildId) {
    let activity = ActivityData::custom("Recording...");
    let status = OnlineStatus::DoNotDisturb;
    ctx.set_presence(Some(activity), status);

    let bot_name = ctx.cache.current_user().display_name().to_string();

    guild_id.edit_nickname(ctx, Some(format!("🔴 {bot_name}").as_str())).await.unwrap_or_else(|e| {
        warn!("Failed to set nickname: {e:?}");
    });
}

pub async fn reset_presence(ctx: &Context, guild_id: GuildId) {
    let status = OnlineStatus::Online;
    ctx.set_presence(None, status);

    guild_id.edit_nickname(ctx, None).await.unwrap_or_else(|e| {
        warn!("Failed to set nickname: {e}");
    });
}

pub async fn get_channel_or_default_current(ctx: &Context, cmd: &CommandInteraction) -> Option<ChannelId> {
    let guild_id = cmd.guild_id.unwrap();
    let channel_opt = cmd.data.options.first();
    match channel_opt {
        Some(x) => {
            Some(x.value.as_channel_id().unwrap())
        }
        None => {
            get_member_channel(ctx, guild_id, cmd.user.id).await
        }
    }
}

pub async fn get_member_channel(ctx: &Context, guild_id: GuildId, user_id: UserId) -> Option<ChannelId> {
    let guild_channels = match guild_id.channels(ctx).await {
        Ok(x) => x,
        Err(e) => {
            error!("Failed to retrieve guild channels: {e:?}");
            return None;
        }
    };

    let voice_channels = guild_channels.iter().filter(|x| x.1.kind == ChannelType::Voice ).collect::<Vec<_>>();

    for channel in voice_channels {
        let members = match channel.1.members(ctx) {
            Ok(x) => x,
            Err(e) => {
                error!("Failed to retrieve members from channel {}: {e:?}", channel.0);
                return None;
            }
        };

        if let Some(_) = members.iter().find(|f| f.user.id == user_id) {
            return Some(*channel.0);
        }
    }

    None
}