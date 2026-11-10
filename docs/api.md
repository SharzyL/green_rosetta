FORMAT: 1A

# Green Rosetta API

Green Rosetta is a music radio streaming service that serves a dynamic multi-period DASH MPD
and static CMAF media.

This document describes the HTTP API in API Blueprint format.

## Host

The API is served by the Cloudflare Worker, on the same origin as the web player.

+ Host: `http://127.0.0.1:8787`

## Notes

- Streaming clients should load `GET /stream/manifest.mpd` and then follow the segment URLs
  embedded in the MPD.
- MPD `initialization` and `media` URLs, and every `cover_url`, point at `PUBLIC_MEDIA_URL` (the
  bucket's public origin) and are fetched directly from there. They are URL-encoded by the worker.
- Unknown browser navigations are redirected to `/404/` (the dedicated 404 entrypoint).
- Errors are plain-text bodies.

# Group Health

## Health Check [/health]

### Check server health [GET]

+ Response 200 (text/plain)

        OK

# Group Time

## UTC Timing [/api/utc]

Used as a DASH `UTCTiming` time source. Returns a plain-text `xs:dateTime` body.

### Get UTC time [GET]

+ Response 200 (text/plain; charset=utf-8)
    + Headers

            Cache-Control: no-store

    + Body

            2026-01-17T15:30:45.123Z

## Time Sync [/api/sync/time]

Helper endpoint for client/server time sync experiments.

### Get server time [GET]

+ Response 200 (application/json)
    + Body

            {
              "timestamp": "2026-01-17T15:30:45.123Z",
              "unix_millis": 1737044445123
            }

# Group Streaming

## DASH Manifest [/stream/manifest.mpd]

Dynamic DASH MPD with multiple Periods, one per track in the current scheduler window.

### Get MPD [GET]

+ Response 200 (application/dash+xml)
    + Headers

            Cache-Control: no-cache

    + Body

            <?xml version="1.0" encoding="UTF-8"?>
            <MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
                 type="dynamic"
                 availabilityStartTime="2026-01-20T00:00:00.000Z"
                 publishTime="2026-01-20T00:00:10.000Z"
                 timeShiftBufferDepth="PT30S"
                 suggestedPresentationDelay="PT10S">
              <UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014" value="/api/utc" />
              <!-- Periods omitted -->
            </MPD>

# Group Metadata

## Now Playing [/api/now{?t}]

What is on air at a point on the presentation timeline (seconds since the MPD's
`availabilityStartTime`). Clients pass their own playhead so the answer matches what they hear.

+ Parameters
    + t: `1234.5` (number, optional) - Timeline position; defaults to the live edge

### Get the scheduled track [GET]

+ Response 200 (application/json)
    + Body

            {
              "track_id": "662b7d4cd5",
              "album_id": "35f03850c0",
              "period_start_seconds": 1200.0,
              "duration_seconds": 39.96
            }

+ Response 400 (text/plain)

        Invalid t: abc

+ Response 404 (text/plain)

        nothing scheduled at t=-100

## Track Info [/api/tracks/{track_id}]

Query track metadata by `track_id`, e.g. the one `/api/now` returned (Period ids are
`period_{track_id}`).

+ Parameters
    + track_id: `662b7d4cd5` (string) - Track ID

### Get track info [GET]

+ Response 200 (application/json)
    + Body

            {
              "album_id": "35f03850c0",
              "album_name": "Kikuo Miku 7",
              "album_artist": "Kikuo",
              "cover_url": "https://media.example.com/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg",
              "track_id": "662b7d4cd5",
              "track_title": "Twins at the carousel",
              "track_artist": "Kikuo",
              "track_number": 1,
              "disc_number": 1,
              "duration_seconds": 39.96
            }

+ Response 404 (text/plain)

        Track not found

## Album Info [/api/albums/{album_id}]

Query album metadata and track list by `album_id`.

+ Parameters
    + album_id: `35f03850c0` (string) - Album ID

### Get album info [GET]

+ Response 200 (application/json)
    + Body

            {
              "album_id": "35f03850c0",
              "album_name": "Kikuo Miku 7",
              "album_artist": "Kikuo",
              "release_date": "2023-03-24",
              "original_release_date": null,
              "album_label": "Some Label",
              "album_media": "CD",
              "album_catalog_number": "SYWC-7814, SVWC-7815",
              "musicbrainz_album_id": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
              "disc_count": 1,
              "cover_url": "https://media.example.com/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg",
              "tracks": [
                {
                  "track_id": "662b7d4cd5",
                  "track_title": "Twins at the carousel",
                  "track_artist": "Kikuo",
                  "track_number": 1,
                  "disc_number": 1,
                  "duration_seconds": 39.96
                }
              ]
            }

+ Response 404 (text/plain)

        Album not found

# Group Admin

Admin endpoints are cookie-authenticated. Sessions, like album toggles, are kept in the station's
Durable Object storage and survive restarts. All admin endpoints share a rate limit
(`ADMIN_RATE_LIMIT_PER_SECOND`) and answer `429 Rate limit exceeded` beyond it.

- Cookie name: `gr_admin_session`
- Cookie flags: `HttpOnly; SameSite=Lax; Path=/api/admin`

## Login [/api/admin/login]

### Log in [POST]

+ Request (application/json)
    + Body

            {
              "username": "admin",
              "password": "your-password"
            }

+ Response 200 (application/json)
    + Headers

            Set-Cookie: gr_admin_session=...; HttpOnly; SameSite=Lax; Path=/api/admin; Max-Age=86400

    + Body

            { "ok": true }

+ Response 401 (text/plain)

        Invalid credentials

When `SCRUTINIZER_USERNAME` or `SCRUTINIZER_PASSWORD_HASH` is unset, every login is rejected.

## Logout [/api/admin/logout]

### Log out [POST]

+ Response 200 (application/json)
    + Headers

            Set-Cookie: gr_admin_session=; HttpOnly; SameSite=Lax; Path=/api/admin; Max-Age=0

    + Body

            { "ok": true }

## Session Check [/api/admin/me]

### Check session [GET]

+ Response 200 (application/json)
    + Body

            { "ok": true }

+ Response 401 (text/plain)

        Not logged in

## Albums [/api/admin/albums]

### List albums [GET]

+ Request
    + Headers

            Cookie: gr_admin_session=...

+ Response 200 (application/json)
    + Body

            [
              {
                "album_id": "35f03850c0",
                "album_name": "Kikuo Miku 7",
                "album_artist": "Kikuo",
                "enabled": true,
                "track_count": 12,
                "cover_url": "https://media.example.com/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg"
              }
            ]

+ Response 401 (text/plain)

        Not logged in

## Album Enabled [/api/admin/albums/{album_id}/enabled]

+ Parameters
    + album_id: `35f03850c0` (string) - Album ID

### Set enabled state [PUT]

+ Request (application/json)
    + Headers

            Cookie: gr_admin_session=...

    + Body

            { "enabled": true }

+ Response 200 (application/json)
    + Body

            {
              "album_id": "35f03850c0",
              "album_name": "Kikuo Miku 7",
              "album_artist": "Kikuo",
              "enabled": true,
              "track_count": 12,
              "cover_url": "https://media.example.com/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg"
            }

+ Response 401 (text/plain)

        Not logged in

+ Response 404 (text/plain)

        Album not found

## Reload [/api/admin/reload]

### Reload metadata [POST]

Re-read `metadata/*.yaml` from the bucket. Toggles carry over by album id; the schedule is kept, so
listeners' timeline stays continuous. On failure the current catalogue keeps serving.

+ Request
    + Headers

            Cookie: gr_admin_session=...

+ Response 200 (application/json)
    + Body

            {
              "albums_before": 19,
              "albums_after": 20,
              "added": 1,
              "removed": 0,
              "kept_disabled": 2
            }

+ Response 409 (text/plain)

        reload already in progress

+ Response 500 (text/plain)

        reload failed: Invalid album metadata in 'metadata/x.yaml': ...
