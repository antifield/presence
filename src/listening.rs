//! Normalises Discord "Listening to" activities (type 2).
//!
//! Discord's own Spotify integration and Rich Presence apps (PreMiD's
//! SoundCloud, etc.) all report as `ActivityType::Listening`, but only
//! Spotify's cover keys point at Spotify's CDN. Everything here is pure so it
//! can be tested against raw gateway payloads.

use serenity::all::{Activity, ActivityType};

use crate::consts::listening as listening_cfg;
use crate::{ListeningActivity, ListeningSource};

/// Build the public URL for an activity's `assets.large_image` key.
///
/// Every field of an activity is chosen by whoever publishes it, so only keys
/// in the exact shape Discord delivers are turned into URLs, and only on
/// Spotify's or Discord's own image hosts:
///
/// - `spotify:<40 hex>` -> `https://i.scdn.co/image/<id>`
/// - `mp:<path>` -> `https://media.discordapp.net/<path>` (Discord media proxy)
/// - a snowflake asset id -> `https://cdn.discordapp.com/app-assets/<application_id>/<key>.png`
/// - anything else (including raw `http(s)` URLs, which the gateway never
///   delivers) or no key -> `None`
pub fn resolve_art_url(large_image: Option<&str>, application_id: Option<&str>) -> Option<String> {
    let key = large_image?;
    if let Some(id) = key.strip_prefix("spotify:") {
        let valid = id.len() == 40
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        return valid.then(|| format!("https://i.scdn.co/image/{id}"));
    }
    if let Some(path) = key.strip_prefix("mp:") {
        let valid = !path.is_empty()
            && path
                .chars()
                .all(|c| c.is_ascii_graphic() && !"\"'<>\\`{}|^".contains(c));
        return valid.then(|| format!("https://media.discordapp.net/{path}"));
    }
    let app_id = application_id?;
    let valid = (17..=20).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_digit());
    valid.then(|| format!("https://cdn.discordapp.com/app-assets/{app_id}/{key}.png"))
}

/// Make user-supplied activity text safe to show as plain text: drop control,
/// bidi-override and invisible characters, cap the length, and treat an empty
/// result as missing.
fn clean_text(text: Option<&str>) -> Option<String> {
    let cleaned: String = text?
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(
                    c,
                    '\u{200B}' | '\u{2060}' | '\u{FEFF}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
                )
        })
        .take(listening_cfg::MAX_TEXT_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Discord's own Spotify activity, as opposed to an app that happens to be
/// called "Spotify". Only the real one carries a `sync_id` (the track id).
fn is_spotify(activity: &Activity) -> bool {
    activity.name == "Spotify" && activity.sync_id.is_some()
}

fn source(activity: &Activity) -> ListeningSource {
    if is_spotify(activity) {
        ListeningSource::Spotify
    } else if activity.name.eq_ignore_ascii_case("SoundCloud") {
        ListeningSource::Soundcloud
    } else {
        ListeningSource::Other
    }
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
        let track_id = a.sync_id.clone().filter(|id| {
            is_spotify(a) && id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())
        });

        ListeningActivity {
            source: source(a),
            art_url: resolve_art_url(
                assets.and_then(|asst| asst.large_image.as_deref()),
                application_id.as_deref(),
            ),
            application_id,
            track: clean_text(a.details.as_deref()),
            artist: clean_text(a.state.as_deref()),
            album: clean_text(assets.and_then(|asst| asst.large_text.as_deref())),
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
            "sync_id": "049iW2TiA4H0NnGvVMibOh",
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
    fn art_url_rejects_raw_http_urls() {
        for url in [
            "https://i1.sndcdn.com/artworks-t500x500.jpg",
            "http://example.com/cover.png",
        ] {
            assert_eq!(resolve_art_url(Some(url), Some("802958833214423081")), None);
        }
    }

    #[test]
    fn art_url_rejects_malformed_keys() {
        for key in [
            "spotify:",
            "spotify:AB67616D0000B273BB86AA29F862C224E21B96D8",
            "spotify:ab67616d0000b273bb86aa29f862c224e21b96d8/../x",
            "mp:",
            "mp:external/x\" onerror=\"alert(1)",
            "mp:external/<script>",
            "mp:external/a b",
            "large_cover",
            "123",
            "1093518318284009472.png",
        ] {
            assert_eq!(
                resolve_art_url(Some(key), Some("802958833214423081")),
                None,
                "{key}"
            );
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
                "source": "soundcloud",
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
    fn spotify_listening_frame() {
        let listening = ListeningActivity::from(&spotify());
        assert_eq!(
            serde_json::to_value(&listening).unwrap(),
            json!({
                "source": "spotify",
                "application_id": null,
                "track": "A Shoulder to Cry On",
                "artist": "Dance Gavin Dance",
                "album": "Pantheon",
                "art_url": "https://i.scdn.co/image/ab67616d0000b273bb86aa29f862c224e21b96d8",
                "track_id": "049iW2TiA4H0NnGvVMibOh",
                "url": "https://open.spotify.com/track/049iW2TiA4H0NnGvVMibOh",
                "started_at_ms": 1766447419972_i64,
                "ends_at_ms": 1766447701646_i64
            })
        );
    }

    #[test]
    fn text_is_cleaned_and_capped() {
        assert_eq!(
            clean_text(Some("  evil\u{202E}gnos\u{200B}\n\u{0007} ")),
            Some("evilgnos".into())
        );
        assert_eq!(
            clean_text(Some("藍月なくる 👨‍👩‍👧")),
            Some("藍月なくる 👨‍👩‍👧".into())
        );
        assert_eq!(clean_text(Some("\u{200B}\u{FEFF} ")), None);
        assert_eq!(clean_text(None), None);
        let long = "あ".repeat(500);
        assert_eq!(clean_text(Some(&long)).unwrap().chars().count(), 128);
    }

    #[test]
    fn malformed_sync_id_gets_no_track_link() {
        let mut fake = spotify();
        fake.sync_id = Some("x\"><img src=x>".into());
        let listening = ListeningActivity::from(&fake);
        assert_eq!(listening.source, ListeningSource::Spotify);
        assert_eq!(listening.track_id, None);
        assert_eq!(listening.url, None);
    }

    #[test]
    fn unrecognised_app_is_other_source() {
        let listening = ListeningActivity::from(&listening_at("Apple Music", Some(1)));
        assert_eq!(listening.source, ListeningSource::Other);
        assert_eq!(
            serde_json::to_value(listening.source).unwrap(),
            json!("other")
        );
    }

    #[test]
    fn app_named_spotify_without_sync_id_is_other_with_no_track_link() {
        let listening = ListeningActivity::from(&listening_at("Spotify", Some(1)));
        assert_eq!(listening.source, ListeningSource::Other);
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
