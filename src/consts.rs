//! Centralized configuration constants and env-var defaults.
//!
//! Grouped into nested modules by subject so call sites read like
//! `ttl::PRESENCE_MS`, `http::LISTEN_PORT`, `ws::PING_INTERVAL`, etc.

use std::time::Duration;

/// Cache time-to-live tunables.
///
/// Two flavours of cache:
/// - **Presence** — Redis-side TTL set via `SET EX`; the same value is
///   re-asserted client-side via [`PRESENCE_MS`] when serving snapshots.
/// - **Membership** — split TTL by polarity so positive results stay warm
///   (gateway events keep them fresh) and negative results re-check the
///   API soon after a miss.
pub mod ttl {
    /// Presence Redis TTL.
    pub const PRESENCE_SECS: u64 = 300;
    /// Presence soft staleness threshold (matches Redis TTL).
    pub const PRESENCE_MS: i64 = (PRESENCE_SECS as i64) * 1000;

    /// `in_server == true` Redis TTL.
    pub const MEMBERSHIP_POSITIVE_SECS: u64 = 6 * 60 * 60;
    /// `in_server == false` Redis TTL.
    pub const MEMBERSHIP_NEGATIVE_SECS: u64 = 5 * 60;
    pub const MEMBERSHIP_POSITIVE_MS: i64 = (MEMBERSHIP_POSITIVE_SECS as i64) * 1000;
    pub const MEMBERSHIP_NEGATIVE_MS: i64 = (MEMBERSHIP_NEGATIVE_SECS as i64) * 1000;
}

/// HTTP server tunables.
pub mod http {
    pub const LISTEN_HOST: [u8; 4] = [0, 0, 0, 0];
    pub const LISTEN_PORT: u16 = 8787;
    pub const MAX_CONNECTIONS_PER_IP: usize = 10;
}

/// WebSocket tunables.
pub mod ws {
    use super::Duration;

    pub const SEND_TIMEOUT: Duration = Duration::from_secs(5);
    pub const PING_INTERVAL: Duration = Duration::from_secs(25);
}

/// Redis bootstrap behaviour at startup.
pub mod redis_boot {
    use super::Duration;

    pub const TIMEOUT: Duration = Duration::from_secs(10);
    pub const RETRY: Duration = Duration::from_millis(200);
}

/// Discord-side defaults.
pub mod discord {
    /// Compile-time fallback for the `GUILD_ID` env var. Hardcoded to the
    /// Antifield discord; override with the env var if needed.
    pub const DEFAULT_GUILD_ID: Option<u64> = Some(982_385_887_000_272_956);
}
