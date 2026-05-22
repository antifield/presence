//! Two-tier cache: Redis primary, in-memory fallback.
//!
//! Both the presence cache (`presence:{user_id}`) and the membership cache
//! (`in_server:{user_id}`) share the same shape:
//!
//! 1. Try Redis. A connection error / decode error falls through to the
//!    in-memory map; an explicit `Ok(None)` is treated as a definitive miss.
//! 2. The in-memory map enforces its own TTL on read (Redis manages TTL
//!    itself via `SET EX`).
//!
//! All of that lives behind [`Cache`] so call sites don't repeat the dance.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use tokio::sync::OnceCell;
use tracing::{info, warn};

use crate::PresenceData;
use crate::consts::{redis_boot, ttl};

static REDIS_CLIENT: OnceCell<Option<ConnectionManager>> = OnceCell::const_new();

pub async fn init_redis() -> bool {
    let result = REDIS_CLIENT
        .get_or_init(|| async {
            let url = match std::env::var("REDIS_URL") {
                Ok(u) => u,
                Err(_) => {
                    info!("REDIS_URL not set, using in-memory cache");
                    return None;
                }
            };

            match redis::Client::open(url.as_str()) {
                Ok(client) => match ConnectionManager::new(client).await {
                    Ok(cm) => {
                        info!("redis connected");
                        Some(cm)
                    }
                    Err(e) => {
                        warn!(?e, "failed to connect to redis, using in-memory cache");
                        None
                    }
                },
                Err(e) => {
                    warn!(?e, "invalid redis url, using in-memory cache");
                    None
                }
            }
        })
        .await;

    result.is_some()
}

pub fn is_redis_available() -> bool {
    REDIS_CLIENT.get().map(|opt| opt.is_some()).unwrap_or(false)
}

async fn get_redis() -> Option<ConnectionManager> {
    REDIS_CLIENT.get()?.clone()
}

/// Outcome of a Redis read, before any in-memory fallback is consulted.
enum RedisRead<T> {
    /// Authoritative hit from Redis.
    Hit(T),
    /// Authoritative miss from Redis (key was unset, not an error).
    Miss,
    /// Redis didn't answer authoritatively — caller should fall back to
    /// the in-memory tier. Wraps an error message for diagnostics.
    Unavailable,
}

/// Read a key from Redis and parse it via the supplied closure.
///
/// Centralizes the "Ok(Some) -> Hit / Ok(None) -> Miss / Err -> Unavailable"
/// dance every cache lookup used to repeat.
async fn redis_read<T>(key: &str, parse: impl FnOnce(String) -> Option<T>) -> RedisRead<T> {
    let Some(mut redis) = get_redis().await else {
        return RedisRead::Unavailable;
    };
    match redis.get::<_, Option<String>>(key).await {
        Ok(Some(raw)) => match parse(raw) {
            Some(v) => RedisRead::Hit(v),
            None => RedisRead::Unavailable,
        },
        Ok(None) => RedisRead::Miss,
        Err(_) => RedisRead::Unavailable,
    }
}

/// Write a string value to Redis with a TTL. Silently swallows errors —
/// the caller is expected to mirror the same value into the in-memory tier
/// so a Redis hiccup doesn't lose data.
async fn redis_set_ex(key: &str, value: &str, ttl_secs: u64) {
    if let Some(mut redis) = get_redis().await {
        let _: Result<(), _> = redis.set_ex(key, value, ttl_secs).await;
    }
}

async fn redis_del(key: &str) {
    if let Some(mut redis) = get_redis().await {
        let _: Result<(), _> = redis.del::<_, ()>(key).await;
    }
}

#[derive(Debug, Clone, Copy)]
struct MembershipEntry {
    in_server: bool,
    cached_at_ms: i64,
}

impl MembershipEntry {
    fn ttl_ms(self) -> i64 {
        if self.in_server {
            ttl::MEMBERSHIP_POSITIVE_MS
        } else {
            ttl::MEMBERSHIP_NEGATIVE_MS
        }
    }

    fn fresh(self, now_ms: i64) -> bool {
        now_ms - self.cached_at_ms <= self.ttl_ms()
    }
}

pub struct Cache {
    memory_presence: Arc<DashMap<String, PresenceData>>,
    memory_membership: Arc<DashMap<String, MembershipEntry>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            memory_presence: Arc::new(DashMap::new()),
            memory_membership: Arc::new(DashMap::new()),
        }
    }

    // -- presence -----------------------------------------------------------

    pub async fn get(&self, user_id: &str) -> Option<PresenceData> {
        let key = presence_key(user_id);
        match redis_read(&key, |raw| serde_json::from_str(&raw).ok()).await {
            RedisRead::Hit(presence) => Some(presence),
            RedisRead::Miss => None,
            RedisRead::Unavailable => self.memory_presence.get(user_id).map(|r| r.clone()),
        }
    }

    pub async fn set(&self, user_id: &str, data: &PresenceData) {
        let key = presence_key(user_id);
        if let Ok(json) = serde_json::to_string(data) {
            redis_set_ex(&key, &json, ttl::PRESENCE_SECS).await;
        }
        self.memory_presence
            .insert(user_id.to_string(), data.clone());
    }

    pub async fn remove(&self, user_id: &str) {
        redis_del(&presence_key(user_id)).await;
        self.memory_presence.remove(user_id);
    }

    // -- membership ---------------------------------------------------------

    pub async fn get_membership(&self, user_id: &str) -> Option<bool> {
        let key = membership_key(user_id);
        match redis_read(&key, |raw| Some(raw == "1")).await {
            RedisRead::Hit(in_server) => Some(in_server),
            RedisRead::Miss => None,
            RedisRead::Unavailable => self.read_membership_memory(user_id),
        }
    }

    pub async fn set_membership(&self, user_id: &str, in_server: bool) {
        let ttl_secs = if in_server {
            ttl::MEMBERSHIP_POSITIVE_SECS
        } else {
            ttl::MEMBERSHIP_NEGATIVE_SECS
        };
        let value = if in_server { "1" } else { "0" };
        redis_set_ex(&membership_key(user_id), value, ttl_secs).await;
        self.memory_membership.insert(
            user_id.to_string(),
            MembershipEntry {
                in_server,
                cached_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
    }

    fn read_membership_memory(&self, user_id: &str) -> Option<bool> {
        let entry = self.memory_membership.get(user_id).map(|r| *r)?;
        let now = chrono::Utc::now().timestamp_millis();
        if entry.fresh(now) {
            Some(entry.in_server)
        } else {
            self.memory_membership.remove(user_id);
            None
        }
    }
}

fn presence_key(user_id: &str) -> String {
    format!("presence:{}", user_id)
}

fn membership_key(user_id: &str) -> String {
    format!("in_server:{}", user_id)
}

pub async fn wait_for_redis(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if init_redis().await {
            return true;
        }
        tokio::time::sleep(redis_boot::RETRY).await;
    }
    false
}
