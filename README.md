# Presence

Presence is a service for getting the current Spotify/Listening status of users in our [Discord Server](https://discord.gg/noid). It connects a Discord Bot (with Presence Intent) to the Gateway and exposes it via REST and WebSocket.

> [!NOTE]
> Presence is only available for users in our server.
> You can try this for yourself by joining our [Discord](https://discord.gg/noid).

## Endpoints

- WebSocket stream: `WS /ws/v1/{DISCORD_USER_ID}` (personally use `websocat` to test in dev)
- REST snapshot: `GET /v1/{DISCORD_USER_ID}` (returns the latest cached presence, served from Redis whenever the gateway has seen the user)
- Check if user in server: `GET /v1/{DISCORD_USER_ID}/in_server` (served from Redis; falls back to the Discord REST API on cache miss and writes the result back through)
- Health: `GET /health`

## Usage

### Response

The REST snapshot and every WebSocket frame share this shape:

```json
{
  "user_id": "492731761680187403",
  "listening": {
    "source": "SoundCloud",
    "application_id": "802958833214423081",
    "track": "Aitsuki Nakuru - Monochrome Butterfly",
    "artist": "藍月なくる",
    "album": null,
    "art_url": "https://media.discordapp.net/external/kvJxiMrHhX9w-Ts0UnAdHQUAdN7ScthPDvAXWR92JMo/https/i1.sndcdn.com/artworks-000200210160-fa6dq4-t500x500.jpg",
    "track_id": null,
    "url": null,
    "started_at_ms": 1792367951000,
    "ends_at_ms": 1792368147000
  },
  "spotify": {
    "track": "Aitsuki Nakuru - Monochrome Butterfly",
    "artist": "藍月なくる",
    "album": null,
    "album_art_url": "https://media.discordapp.net/external/kvJxiMrHhX9w-Ts0UnAdHQUAdN7ScthPDvAXWR92JMo/https/i1.sndcdn.com/artworks-000200210160-fa6dq4-t500x500.jpg",
    "started_at_ms": 1792367951000,
    "ends_at_ms": 1792368147000
  },
  "timestamp_ms": 1792367951995
}
```

Both `listening` and `spotify` are `null` when the user isn't listening to anything.

#### `listening`

Built from the user's Discord "Listening to" activity (type 2), whichever app reports it: Spotify, SoundCloud through PreMiD, and so on. If there are several, Discord's own Spotify activity wins, then the one that started most recently.

| Field | Value |
| --- | --- |
| `source` | The activity's `name`, e.g. `"Spotify"`, `"SoundCloud"` |
| `application_id` | The Discord application behind the activity, or `null` (Spotify has none) |
| `track` / `artist` | The activity's `details` / `state` |
| `album` | `assets.large_text`, or `null` |
| `art_url` | Cover image URL, see below, or `null` |
| `track_id` | Spotify only: the Spotify track id (`sync_id`); otherwise `null` |
| `url` | Spotify only: `https://open.spotify.com/track/<track_id>`; otherwise `null` |
| `started_at_ms` / `ends_at_ms` | Activity timestamps, or `null` |

`art_url` is built from `assets.large_image` by prefix (the service never fetches or proxies images):

| `large_image` | `art_url` |
| --- | --- |
| `spotify:<id>` | `https://i.scdn.co/image/<id>` |
| `mp:<path>` | `https://media.discordapp.net/<path>` |
| `http(s)://...` | unchanged |
| any other key | `https://cdn.discordapp.com/app-assets/<application_id>/<key>.png` |
| missing | `null` |

#### `spotify` (deprecated)

> [!WARNING]
> `spotify` is deprecated in favour of `listening` and kept only for clients that already read it. It is filled from the same activity as `listening`, including non-Spotify sources, with `album_art_url` equal to `listening.art_url`.

## Development

```bash
# copy env, fill in credentials
cp .env.example .env

# (DOCKER IS NEEDED) start redis + run the app
./dev.sh
```

### Caching

Presence uses Redis for caching with automatic fallback to in-memory if Redis is unavailable. On startup, the app waits up to 10 seconds for Redis before falling back.

Two things are cached:

- **Listening presence** (`presence:{user_id}`, 5 minute TTL) — populated from gateway `presence_update` events. The REST snapshot endpoint works without an active WebSocket subscriber.
- **Guild membership** (`in_server:{user_id}`) — positive entries kept for 6 hours, negative entries for 5 minutes. Maintained in real-time by the `GUILD_MEMBERS` gateway events (`guild_member_addition` / `guild_member_removal`); a cache miss falls back to a single Discord REST lookup and writes the result back. The bot needs both the `GUILD_MEMBERS` and `GUILD_PRESENCES` privileged intents enabled in the Discord developer portal.

Check `/health` to see current Redis status:
```json
{"status": "ok", "redis": true}
```

## Why this approach?

This avoids dealing with Spotify OAuth in general while still providing real-time listening data, within Discord’s limitations.

This idea for the implementation is heavily inspired by [Lanyard](https://github.com/Phineas/lanyard) which I (dromzeh) personally use on my [own site](https://dromzeh.dev/) to display what I'm currently listening to.

## License

[antifield/presence](https://github.com/antifield/presence) is licensed under the [GNU Affero General Public License v3.0](LICENSE). Authored by [@dromzeh](https://dromzeh.dev/).

You must state all significant changes made to the original software, make the source code available to the public with credit to the original author, original source, and use the same license.

> © 2025 Antifield LTD | Registered UK Company No. 15988228 | ICO Reference No. ZB857511
