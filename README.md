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

```json
{
  "user_id": "492731761680187403",
  "spotify": {
    "track": "A Shoulder to Cry On",
    "artist": "Dance Gavin Dance",
    "album": "Pantheon",
    "album_art_url": "https://i.scdn.co/image/ab67616d0000b273bb86aa29f862c224e21b96d8",
    "started_at_ms": 1766447419972,
    "ends_at_ms": 1766447701646
  },
  "timestamp_ms": 1766447420190
}
```

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

- **Spotify presence** (`presence:{user_id}`, 5 minute TTL) — populated from gateway `presence_update` events. The REST snapshot endpoint works without an active WebSocket subscriber.
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
