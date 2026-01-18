FORMAT: 1A

# Green Rosetta API

Green Rosetta is a music radio streaming service that serves a dynamic multi-period DASH MPD
and static CMAF media.

This document describes the HTTP API in API Blueprint format.

## Host

The base URL is configured via `server.base_url` in `backend.config.toml`.

+ Host: `http://127.0.0.1:8080`

## Notes

- Streaming clients should load `GET /stream/manifest.mpd` and then follow the segment URLs
  embedded in the MPD.
- MPD `initialization` and `media` URLs are URL-encoded by the backend.
- In local mode, the backend serves `/media/*`.
- In S3 mode, segment URLs in the MPD point at `storage.s3.public_access_domain` and are fetched
  directly from that domain; the backend still serves the metadata and admin APIs.
- When the backend is started with `--web-dir`, unknown browser navigations are redirected to
  `/404/` (the dedicated 404 entrypoint).

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

            Cache-Control: max-age=2

    + Body

            <?xml version="1.0" encoding="UTF-8"?>
            <MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
                 type="dynamic"
                 availabilityStartTime="2026-01-20T00:00:00.000Z"
                 publishTime="2026-01-20T00:00:10.000Z"
                 timeShiftBufferDepth="PT30S"
                 suggestedPresentationDelay="PT10S">
              <UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014" value="http://127.0.0.1:8080/api/utc" />
              <!-- Periods omitted -->
            </MPD>

# Group Metadata

## Track Info [/api/tracks/{track_id}]

Query track metadata by `track_id`. The frontend uses this when it learns the active Period id
from dash.js (Period ids are `period_{track_id}`).

+ Parameters
    + track_id: `662b7d4cd5` (string) - Track ID

### Get track info [GET]

+ Response 200 (application/json)
    + Body

            {
              "album_id": "35f03850c0",
              "album_name": "Kikuo Miku 7",
              "album_artist": "Kikuo",
              "cover_url": "/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg",
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
              "cover_url": "/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg",
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

# Group Media

## Media Files [/media/{path}]

Served only in local storage mode. In S3 mode, clients fetch media directly from the public domain
URLs embedded in the MPD.

+ Parameters
    + path: `albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg` (string) - URL-encoded media path

### Get media bytes [GET]

+ Response 200
    + Headers

            Content-Type: image/jpeg

+ Response 404 (text/plain)

        Not Found

# Group Admin

Admin endpoints are cookie-authenticated. Sessions are stored in memory (not persisted).

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

+ Response 503 (text/plain)

        Admin authentication is not configured

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
                "cover_url": "/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg"
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
              "cover_url": "/media/albums/35f03850c0%20-%20Kikuo%20-%20Kikuo_Miku_7/cover.jpg"
            }

+ Response 401 (text/plain)

        Not logged in

+ Response 404 (text/plain)

        Album not found

