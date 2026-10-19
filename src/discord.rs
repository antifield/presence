use std::time::Duration;

use serenity::all::{
    Client, Context, EventHandler, GatewayIntents, Guild, Member, Presence, Ready, ResumedEvent,
    UnavailableGuild, User,
};
use serenity::async_trait;
use serenity::http::Http as SerenityHttp;
use serenity::model::id::{GuildId, UserId};
use tracing::{debug, error, info, warn};

use crate::{ListeningActivity, PresenceCache, PresenceData, UserWatchers, listening};

pub struct Handler {
    pub cache: PresenceCache,
    pub watchers: UserWatchers,
    pub guild_id: GuildId,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        info!(user = %ready.user.name, "discord gateway connected");
    }

    async fn resume(&self, _ctx: Context, _: ResumedEvent) {
        info!("discord gateway resumed");
    }

    async fn guild_create(&self, ctx: Context, guild: Guild, _is_new: Option<bool>) {
        if guild.id == self.guild_id {
            return;
        }
        // The bot should only ever live in the configured guild. If we end up
        // anywhere else (someone invited the bot to their server), leave
        // immediately so we don't broadcast or cache data for users outside
        // our scope and don't spend compute on guilds we don't intend to serve.
        warn!(
            guild_id = %guild.id,
            guild_name = %guild.name,
            "joined unconfigured guild, leaving"
        );
        if let Err(err) = ctx.http.leave_guild(guild.id).await {
            warn!(?err, guild_id = %guild.id, "failed to leave unconfigured guild");
        }
    }

    async fn guild_delete(
        &self,
        _ctx: Context,
        incomplete: UnavailableGuild,
        _full: Option<Guild>,
    ) {
        if incomplete.id == self.guild_id && !incomplete.unavailable {
            warn!(
                guild_id = %incomplete.id,
                "bot removed from configured guild — membership cache will lazy-refill"
            );
        }
    }

    async fn guild_member_addition(&self, _ctx: Context, new_member: Member) {
        if new_member.guild_id != self.guild_id {
            return;
        }
        let user_id = new_member.user.id.to_string();
        debug!(user_id = %user_id, "guild_member_addition");
        self.cache.set_membership(&user_id, true).await;
    }

    async fn guild_member_removal(
        &self,
        _ctx: Context,
        guild_id: GuildId,
        user: User,
        _member_data_if_available: Option<Member>,
    ) {
        if guild_id != self.guild_id {
            return;
        }
        let user_id = user.id.to_string();
        debug!(user_id = %user_id, "guild_member_removal");
        self.cache.set_membership(&user_id, false).await;
    }

    async fn presence_update(&self, _ctx: Context, new: Presence) {
        // Discord delivers presence updates for every guild the bot is in.
        // Ignore anything outside the configured guild so we never cache or
        // broadcast out-of-scope presence data.
        if new.guild_id != Some(self.guild_id) {
            return;
        }

        let user_id = new.user.id.to_string();

        let raw_listening_activity = listening::pick(&new.activities);

        if let Some(a) = raw_listening_activity {
            debug!(user_id = %user_id, activity = ?a, "listening activity");
        }

        let presence = PresenceData {
            user_id: user_id.clone(),
            listening: raw_listening_activity.map(ListeningActivity::from),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };

        // Cache every presence update so the REST snapshot works without an
        // active websocket subscriber.
        self.cache.set(&user_id, &presence).await;

        // Receiving a presence update is positive proof the user is in the
        // guild — refresh the membership cache opportunistically.
        self.cache.set_membership(&user_id, true).await;

        if let Some(watcher) = self.watchers.get(&user_id) {
            let _ = watcher.send(Some(presence));
        }
    }
}

pub async fn start_discord(cache: PresenceCache, watchers: UserWatchers, guild_id: GuildId) -> ! {
    let token = std::env::var("DISCORD_BOT_TOKEN").expect("DISCORD_BOT_TOKEN not set");
    let intents =
        GatewayIntents::GUILDS | GatewayIntents::GUILD_MEMBERS | GatewayIntents::GUILD_PRESENCES;

    let mut attempt: u32 = 0;

    loop {
        let handler = Handler {
            cache: cache.clone(),
            watchers: watchers.clone(),
            guild_id,
        };

        match Client::builder(&token, intents)
            .event_handler(handler)
            .await
        {
            Ok(mut client) => {
                attempt = 0;
                info!("discord client starting");
                if let Err(err) = client.start().await {
                    warn!(?err, "discord client stopped, will restart");
                }
            }
            Err(err) => {
                error!(?err, "failed to create discord client, will retry");
            }
        }

        attempt = attempt.saturating_add(1);
        let backoff_secs = 2_u64.saturating_pow(attempt.min(6)).min(60);
        warn!(attempt, backoff_secs, "reconnecting after backoff");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
    }
}

pub async fn is_member(
    http: &SerenityHttp,
    guild_id: GuildId,
    user_id: u64,
) -> Result<bool, String> {
    match http.get_member(guild_id, UserId::new(user_id)).await {
        Ok(_) => Ok(true),
        Err(err) => {
            if let serenity::Error::Http(http_err) = &err
                && http_err.status_code().map(|s| s.as_u16()) == Some(404)
            {
                return Ok(false);
            }
            Err(format!("discord api error: {:?}", err))
        }
    }
}
