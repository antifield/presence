//! Normalises Discord "Listening to" activities (type 2).
//!
//! Discord's own Spotify integration and Rich Presence apps (PreMiD's
//! SoundCloud, etc.) all report as `ActivityType::Listening`, but only
//! Spotify's cover keys point at Spotify's CDN. Everything here is pure so it
//! can be tested against raw gateway payloads.

use serenity::all::{Activity, ActivityType};

use crate::{ListeningActivity, SpotifyActivity};

/// Build the public URL for an activity's `assets.large_image` key.
///
/// - `spotify:<id>` -> `https://i.scdn.co/image/<id>`
/// - `mp:<path>` -> `https://media.discordapp.net/<path>` (Discord media proxy)
/// - an `http(s)://` URL -> as is
/// - any other key -> `https://cdn.discordapp.com/app-assets/<application_id>/<key>.png`
///   (`None` without an application id)
/// - no key -> `None`
pub fn resolve_art_url(large_image: Option<&str>, application_id: Option<&str>) -> Option<String> {
    let key = large_image.filter(|k| !k.is_empty())?;
    if let Some(id) = key.strip_prefix("spotify:") {
        return Some(format!("https://i.scdn.co/image/{id}"));
    }
    if let Some(path) = key.strip_prefix("mp:") {
        return Some(format!("https://media.discordapp.net/{path}"));
    }
    if key.starts_with("https://") || key.starts_with("http://") {
        return Some(key.to_string());
    }
    application_id.map(|app_id| format!("https://cdn.discordapp.com/app-assets/{app_id}/{key}.png"))
}

/// Discord's own Spotify activity, as opposed to an app that happens to be
/// called "Spotify". Only the real one carries a `sync_id` (the track id).
fn is_spotify(activity: &Activity) -> bool {
    activity.name == "Spotify" && activity.sync_id.is_some()
}

/// Pick the listening activity to report: real Spotify first, then the one
/// that started most recently.
pub fn pick(activities: &[Activity]) -> Option<&Activity> {
    activities
        .iter()
        .filter(|a| a.kind == ActivityType::Listening)
        .max_by_key(|a| (is_spotify(a), a.timestamps.as_ref().and_then(|t| t.start)))
}

impl From<&Activity> for ListeningActivity {
    fn from(a: &Activity) -> Self {
        let application_id = a.application_id.map(|id| id.to_string());
        let assets = a.assets.as_ref();
        let track_id = a.sync_id.clone().filter(|_| is_spotify(a));

        ListeningActivity {
            source: a.name.clone(),
            art_url: resolve_art_url(
                assets.and_then(|asst| asst.large_image.as_deref()),
                application_id.as_deref(),
            ),
            application_id,
            track: a.details.clone(),
            artist: a.state.clone(),
            album: assets.and_then(|asst| asst.large_text.clone()),
            url: track_id
                .as_ref()
                .map(|id| format!("https://open.spotify.com/track/{id}")),
            track_id,
            started_at_ms: a
                .timestamps
                .as_ref()
                .and_then(|t| t.start.map(|v| v as i64)),
            ends_at_ms: a.timestamps.as_ref().and_then(|t| t.end.map(|v| v as i64)),
        }
    }
}

impl From<&ListeningActivity> for SpotifyActivity {
    fn from(l: &ListeningActivity) -> Self {
        SpotifyActivity {
            track: l.track.clone(),
            artist: l.artist.clone(),
            album: l.album.clone(),
            album_art_url: l.art_url.clone(),
            started_at_ms: l.started_at_ms,
            ends_at_ms: l.ends_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const SOUNDCLOUD_IMAGE: &str = "mp:external/kvJxiMrHhX9w-Ts0UnAdHQUAdN7ScthPDvAXWR92JMo/https/i1.sndcdn.com/artworks-000200210160-fa6dq4-t500x500.jpg";
    const SOUNDCLOUD_ART_URL: &str = "https://media.discordapp.net/external/kvJxiMrHhX9w-Ts0UnAdHQUAdN7ScthPDvAXWR92JMo/https/i1.sndcdn.com/artworks-000200210160-fa6dq4-t500x500.jpg";

    fn activity(raw: Value) -> Activity {
        serde_json::from_value(raw).expect("valid discord activity")
    }

    fn soundcloud() -> Activity {
        activity(json!({
            "type": 2,
            "name": "SoundCloud",
            "details": "Aitsuki Nakuru - Monochrome Butterfly",
            "state": "藍月なくる",
            "assets": { "large_image": SOUNDCLOUD_IMAGE },
            "application_id": "802958833214423081",
            "timestamps": { "start": 1792367951000_u64, "end": 1792368147000_u64 },
            "created_at": 1792367951995_u64
        }))
    }

    fn spotify() -> Activity {
        activity(json!({
            "type": 2,
            "name": "Spotify",
            "details": "A Shoulder to Cry On",
            "state": "Dance Gavin Dance",
            "assets": {
                "large_image": "spotify:ab67616d0000b273bb86aa29f862c224e21b96d8",
                "large_text": "Pantheon"
            },
            "sync_id": "4cOdK2wGLETKBW3PvgPWqT",
            "session_id": "0e5a6b0a3c1b8f4f1e2d3c4b5a697887",
            "party": { "id": "spotify:492731761680187403" },
            "flags": 48,
            "timestamps": { "start": 1766447419972_u64, "end": 1766447701646_u64 },
            "created_at": 1766447420190_u64
        }))
    }

    fn listening_at(name: &str, start: Option<u64>) -> Activity {
        activity(json!({
            "type": 2,
            "name": name,
            "timestamps": { "start": start },
            "created_at": 0
        }))
    }

    #[test]
    fn art_url_spotify_key() {
        assert_eq!(
            resolve_art_url(
                Some("spotify:ab67616d0000b273bb86aa29f862c224e21b96d8"),
                None
            ),
            Some("https://i.scdn.co/image/ab67616d0000b273bb86aa29f862c224e21b96d8".into())
        );
    }

    #[test]
    fn art_url_media_proxy_key() {
        assert_eq!(
            resolve_art_url(Some(SOUNDCLOUD_IMAGE), Some("802958833214423081")),
            Some(SOUNDCLOUD_ART_URL.into())
        );
    }

    #[test]
    fn art_url_http_urls_pass_through() {
        for url in [
            "https://i1.sndcdn.com/artworks-t500x500.jpg",
            "http://example.com/cover.png",
        ] {
            assert_eq!(resolve_art_url(Some(url), Some("1")), Some(url.into()));
        }
    }

    #[test]
    fn art_url_app_asset_key() {
        assert_eq!(
            resolve_art_url(Some("1093518318284009472"), Some("802958833214423081")),
            Some(
                "https://cdn.discordapp.com/app-assets/802958833214423081/1093518318284009472.png"
                    .into()
            )
        );
    }

    #[test]
    fn art_url_app_asset_key_without_application_id() {
        assert_eq!(resolve_art_url(Some("1093518318284009472"), None), None);
    }

    #[test]
    fn art_url_no_large_image() {
        assert_eq!(resolve_art_url(None, Some("802958833214423081")), None);
        assert_eq!(resolve_art_url(Some(""), Some("802958833214423081")), None);
    }

    #[test]
    fn soundcloud_listening_frame() {
        let listening = ListeningActivity::from(&soundcloud());
        assert_eq!(
            serde_json::to_value(&listening).unwrap(),
            json!({
                "source": "SoundCloud",
                "application_id": "802958833214423081",
                "track": "Aitsuki Nakuru - Monochrome Butterfly",
                "artist": "藍月なくる",
                "album": null,
                "art_url": SOUNDCLOUD_ART_URL,
                "track_id": null,
                "url": null,
                "started_at_ms": 1792367951000_i64,
                "ends_at_ms": 1792368147000_i64
            })
        );
    }

    #[test]
    fn soundcloud_legacy_spotify_frame_has_resolved_art() {
        let spotify = SpotifyActivity::from(&ListeningActivity::from(&soundcloud()));
        assert_eq!(
            serde_json::to_value(&spotify).unwrap(),
            json!({
                "track": "Aitsuki Nakuru - Monochrome Butterfly",
                "artist": "藍月なくる",
                "album": null,
                "album_art_url": SOUNDCLOUD_ART_URL,
                "started_at_ms": 1792367951000_i64,
                "ends_at_ms": 1792368147000_i64
            })
        );
    }

    #[test]
    fn spotify_listening_frame() {
        let listening = ListeningActivity::from(&spotify());
        assert_eq!(
            serde_json::to_value(&listening).unwrap(),
            json!({
                "source": "Spotify",
                "application_id": null,
                "track": "A Shoulder to Cry On",
                "artist": "Dance Gavin Dance",
                "album": "Pantheon",
                "art_url": "https://i.scdn.co/image/ab67616d0000b273bb86aa29f862c224e21b96d8",
                "track_id": "4cOdK2wGLETKBW3PvgPWqT",
                "url": "https://open.spotify.com/track/4cOdK2wGLETKBW3PvgPWqT",
                "started_at_ms": 1766447419972_i64,
                "ends_at_ms": 1766447701646_i64
            })
        );
    }

    #[test]
    fn spotify_legacy_frame_unchanged() {
        let spotify = SpotifyActivity::from(&ListeningActivity::from(&spotify()));
        assert_eq!(
            serde_json::to_value(&spotify).unwrap(),
            json!({
                "track": "A Shoulder to Cry On",
                "artist": "Dance Gavin Dance",
                "album": "Pantheon",
                "album_art_url": "https://i.scdn.co/image/ab67616d0000b273bb86aa29f862c224e21b96d8",
                "started_at_ms": 1766447419972_i64,
                "ends_at_ms": 1766447701646_i64
            })
        );
    }

    #[test]
    fn app_named_spotify_without_sync_id_gets_no_track_link() {
        let listening = ListeningActivity::from(&listening_at("Spotify", Some(1)));
        assert_eq!(listening.source, "Spotify");
        assert_eq!(listening.track_id, None);
        assert_eq!(listening.url, None);
    }

    #[test]
    fn pick_prefers_real_spotify_over_newer_activity() {
        let activities = [soundcloud(), spotify()];
        assert_eq!(pick(&activities).unwrap().name, "Spotify");
    }

    #[test]
    fn pick_prefers_most_recently_started() {
        let activities = [
            listening_at("Older", Some(100)),
            listening_at("Newest", Some(300)),
            listening_at("Unknown", None),
            listening_at("Middle", Some(200)),
        ];
        assert_eq!(pick(&activities).unwrap().name, "Newest");
    }

    #[test]
    fn pick_ignores_non_listening_activities() {
        let game = activity(json!({
            "type": 0,
            "name": "Deadlock",
            "timestamps": { "start": 9999999999999_u64 },
            "created_at": 0
        }));
        let custom_status = activity(json!({
            "type": 4,
            "name": "Custom Status",
            "state": "hi",
            "created_at": 0
        }));
        assert!(pick(&[game.clone(), custom_status.clone()]).is_none());
        assert_eq!(
            pick(&[game, soundcloud(), custom_status]).unwrap().name,
            "SoundCloud"
        );
        assert!(pick(&[]).is_none());
    }
}
