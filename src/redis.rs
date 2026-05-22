use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use tokio::sync::OnceCell;
use tracing::{info, warn};

use crate::PresenceData;

const PRESENCE_CACHE_TTL_SECS: u64 = 300;
const MEMBERSHIP_POSITIVE_TTL_SECS: u64 = 6 * 60 * 60;
const MEMBERSHIP_NEGATIVE_TTL_SECS: u64 = 5 * 60;
const MEMBERSHIP_POSITIVE_TTL_MS: i64 = (MEMBERSHIP_POSITIVE_TTL_SECS as i64) * 1000;
const MEMBERSHIP_NEGATIVE_TTL_MS: i64 = (MEMBERSHIP_NEGATIVE_TTL_SECS as i64) * 1000;

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

#[derive(Debug, Clone, Copy)]
struct MembershipEntry {
    in_server: bool,
    cached_at_ms: i64,
}

pub struct Cache {
    memory: Arc<DashMap<String, PresenceData>>,
    memory_membership: Arc<DashMap<String, MembershipEntry>>,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            memory: Arc::new(DashMap::new()),
            memory_membership: Arc::new(DashMap::new()),
        }
    }

    pub async fn get(&self, user_id: &str) -> Option<PresenceData> {
        if let Some(mut redis) = get_redis().await {
            let key = format!("presence:{}", user_id);
            match redis.get::<_, Option<String>>(&key).await {
                Ok(Some(json)) => {
                    if let Ok(data) = serde_json::from_str(&json) {
                        return Some(data);
                    }
                }
                Ok(None) => return None,
                Err(_) => {}
            }
        }

        self.memory.get(user_id).map(|r| r.clone())
    }

    pub async fn set(&self, user_id: &str, data: &PresenceData) {
        if let Some(mut redis) = get_redis().await {
            let key = format!("presence:{}", user_id);
            if let Ok(json) = serde_json::to_string(data) {
                let _: Result<(), _> = redis.set_ex(&key, json, PRESENCE_CACHE_TTL_SECS).await;
            }
        }

        self.memory.insert(user_id.to_string(), data.clone());
    }

    pub async fn remove(&self, user_id: &str) {
        if let Some(mut redis) = get_redis().await {
            let key = format!("presence:{}", user_id);
            let _: Result<(), _> = redis.del(&key).await;
        }

        self.memory.remove(user_id);
    }

    pub async fn get_membership(&self, user_id: &str) -> Option<bool> {
        if let Some(mut redis) = get_redis().await {
            let key = membership_key(user_id);
            match redis.get::<_, Option<String>>(&key).await {
                Ok(Some(v)) => return Some(v == "1"),
                Ok(None) => return None,
                Err(_) => {}
            }
        }

        let entry = self.memory_membership.get(user_id).map(|r| *r)?;
        let now = chrono::Utc::now().timestamp_millis();
        let ttl_ms = if entry.in_server {
            MEMBERSHIP_POSITIVE_TTL_MS
        } else {
            MEMBERSHIP_NEGATIVE_TTL_MS
        };
        if now - entry.cached_at_ms > ttl_ms {
            self.memory_membership.remove(user_id);
            None
        } else {
            Some(entry.in_server)
        }
    }

    pub async fn set_membership(&self, user_id: &str, in_server: bool) {
        let ttl_secs = if in_server {
            MEMBERSHIP_POSITIVE_TTL_SECS
        } else {
            MEMBERSHIP_NEGATIVE_TTL_SECS
        };
        if let Some(mut redis) = get_redis().await {
            let key = membership_key(user_id);
            let val = if in_server { "1" } else { "0" };
            let _: Result<(), _> = redis.set_ex(&key, val, ttl_secs).await;
        }
        self.memory_membership.insert(
            user_id.to_string(),
            MembershipEntry {
                in_server,
                cached_at_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
    }
}

fn membership_key(user_id: &str) -> String {
    format!("in_server:{}", user_id)
}

pub async fn wait_for_redis(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    let retry_delay = Duration::from_millis(200);

    while start.elapsed() < timeout {
        if init_redis().await {
            return true;
        }
        tokio::time::sleep(retry_delay).await;
    }

    false
}
