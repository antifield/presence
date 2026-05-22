//! Centralized configuration constants and env-var defaults.
//!
//! Tunables for cache TTLs, the embedded HTTP server, Redis bootstrap
//! behaviour and Discord defaults live here so we don't have to chase
//! magic numbers across modules.

use std::time::Duration;

// ---------------------------------------------------------------------------
// Cache TTLs
// ---------------------------------------------------------------------------

/// Redis TTL for cached presence payloads.
pub const PRESENCE_CACHE_TTL_SECS: u64 = 300;
/// Soft staleness threshold applied when serving cached presence to clients.
pub const PRESENCE_TTL_MS: i64 = (PRESENCE_CACHE_TTL_SECS as i64) * 1000;

/// Positive `in_server` cache TTL. Long-lived: gateway events keep it fresh.
pub const MEMBERSHIP_POSITIVE_TTL_SECS: u64 = 6 * 60 * 60;
/// Negative `in_server` cache TTL. Short — re-check the API soon after a miss.
pub const MEMBERSHIP_NEGATIVE_TTL_SECS: u64 = 5 * 60;
pub const MEMBERSHIP_POSITIVE_TTL_MS: i64 = (MEMBERSHIP_POSITIVE_TTL_SECS as i64) * 1000;
pub const MEMBERSHIP_NEGATIVE_TTL_MS: i64 = (MEMBERSHIP_NEGATIVE_TTL_SECS as i64) * 1000;

// ---------------------------------------------------------------------------
// Discord
// ---------------------------------------------------------------------------

/// Optional fallback for the `GUILD_ID` env var. Set to `Some(<guild_id>)`
/// to make the env var optional in the canonical Antifield deployment;
/// leave `None` to require explicit configuration (current behaviour).
pub const DEFAULT_GUILD_ID: Option<u64> = None;

// ---------------------------------------------------------------------------
// HTTP server
// ---------------------------------------------------------------------------

pub const LISTEN_HOST: [u8; 4] = [0, 0, 0, 0];
pub const LISTEN_PORT: u16 = 8787;
pub const MAX_CONNECTIONS_PER_IP: usize = 10;
pub const WS_SEND_TIMEOUT: Duration = Duration::from_secs(5);
pub const WS_PING_INTERVAL: Duration = Duration::from_secs(25);

// ---------------------------------------------------------------------------
// Redis bootstrap
// ---------------------------------------------------------------------------

pub const REDIS_BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(10);
pub const REDIS_BOOTSTRAP_RETRY: Duration = Duration::from_millis(200);
