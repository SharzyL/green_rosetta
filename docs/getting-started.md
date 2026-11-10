# Green Rosetta - Getting Started Guide

## Prerequisites

- Rust (or Nix) and FFmpeg 5.0+ with AAC/Opus, for the generator
- Node.js 22+ and pnpm, for the worker and frontend
- A Cloudflare account with an R2 bucket that has a public domain
- Audio files in FLAC format (or other FFmpeg-supported formats)

## Quick Start

### 1. Generate Music Metadata and Segments

```bash
cd green_rosetta

# Process albums from /tank/mus/bandcamp directory
cargo run -p green-rosetta-generator --release -- generate \
  --output-dir ./test_dir \
  --scan /tank/mus/bandcamp \
  --filter "Kikuo"  # Optional: filter by artist/album regex (only with --scan)
```

**Output:**

- `test_dir/metadata/` - Metadata directory (one YAML file per album)
- `test_dir/media/albums/` - Encoded CMAF audio segments

The generator reads `generator.config.toml` (gitignored; start from `docs/generator.config.toml`):

```toml
[streaming]
segment_duration = 6.0

[[audio.adaption_sets]]
codec = "aac"
bitrates = [ "128k", "192k" ]
sample_rate = 48000
channels = 2

[[audio.adaption_sets]]
codec = "opus"
bitrates = [ "64k", "128k" ]
sample_rate = 48000
channels = 2
```

### 2. Upload to the Bucket

```bash
rclone sync ./test_dir r2:rosetta --progress
```

The bucket root must hold `media/` and `metadata/`. See the README for setting up the rclone remote.

### 3. Run the Worker Locally

```bash
pnpm install
pnpm build:frontend

# Admin credentials for local runs
echo "SCRUTINIZER_USERNAME=admin" > .dev.vars
echo "SCRUTINIZER_PASSWORD_HASH=$(pnpm -s hash-password)" >> .dev.vars

pnpm dev
```

The worker listens on http://localhost:8787 and reads the bucket named in `wrangler.toml` (`remote = true`, so
`wrangler login` first). Station state (schedule, album toggles, sessions) is kept in a local Durable Object
under `.wrangler/`.

For frontend work, also run `pnpm dev:frontend`: Vite serves http://localhost:3000 with hot reload and
proxies `/api` and `/stream` to the worker.

### 4. Test the API

```bash
# Check health
curl http://localhost:8787/health

# What is on air now
curl http://localhost:8787/api/now | jq .

# Its track info
curl http://localhost:8787/api/tracks/<track_id> | jq .

# Get server time
curl http://localhost:8787/api/sync/time | jq .

# Get DASH manifest; its segment URLs point at PUBLIC_MEDIA_URL
curl http://localhost:8787/stream/manifest.mpd
```

### 5. Deploy

```bash
pnpm wrangler secret put SCRUTINIZER_USERNAME
pnpm -s hash-password | pnpm wrangler secret put SCRUTINIZER_PASSWORD_HASH
pnpm build:frontend
pnpm deploy
```

## Project Structure

```
green_rosetta/
├── worker/                  # Cloudflare Worker (TypeScript)
│   ├── index.ts            # Entry: routing, clock endpoints, static assets, cache headers
│   ├── station.ts          # Durable Object: schedule, API, admin, persisted state
│   ├── scheduler.ts        # Playlist and manifest window logic (pure)
│   ├── catalogue.ts        # Metadata schema, R2 loading, duration helpers
│   ├── dash.ts             # MPD generation
│   ├── auth.ts             # Admin password and cookie helpers
│   └── test/               # Vitest suites, run in the Workers runtime
├── shared/api.ts            # API types shared by worker and frontend
├── frontend/                # React player (/) and admin UI (/scrutinizer/)
├── generator/               # Rust CLI for audio processing
├── wrangler.toml            # Worker configuration
└── generator.config.toml    # Generator configuration (gitignored)
```

## Metadata Format

Metadata is stored as one YAML file per album under `metadata/`:

```yaml
id: 00254d841f
name: It’s Time
artist: Max Roach, His Chorus and Orchestra
release_date: '1962'
disc_count: 1
cover_path: albums/00254d841f - Max_Roach,_His_Chorus_and_Orchestra - It’s_Time/cover.jpg
created_at: 2026-05-20T11:47:36.280495657+00:00
updated_at: 2026-05-20T11:47:36.280495657+00:00
images:
- id: 2aa72a35ff
  image_path: albums/00254d841f - Max_Roach,_His_Chorus_and_Orchestra - It’s_Time/cover.jpg
  image_type: cover
  is_primary: true
tracks:
- id: 407ad2ba06
  title: It’s Time
  artist: Max Roach, His Chorus and Orchestra
  disc_number: 1
  track_number: 1
  length_seconds: 406.2
  representations:
  - name: as1_aac_128k
    adaptation_set_id: as1_aac
    codec: aac
    mpd_codecs: mp4a.40.2
    bandwidth: 128000
    sample_rate: 48000
    channels: 2
    init_segment_path: albums/00254d841f - …/track_1_1/as1_aac_128k/init.mp4
    segment_path_template: albums/00254d841f - …/track_1_1/as1_aac_128k/chunk_%03d$.m4s
    segment_count: 28
    segment_timescale: 48000
    segment_timeline:
    - t: 0
      d: 719872
    - d: 720896
      r: 25
    - d: 34432
    encoded_length_seconds: 406.2
  created_at: 2026-05-20T11:47:36.280495657+00:00
```

**Key Features:**

- **10-digit hex IDs:** Collision-free unique identifiers
- **Multi-disc support:** `track_{disc}_{track}` keeps paths apart across discs
- **Timeline-based segments:** `segment_timeline` comes from ffmpeg's MPD, so Period durations match the audio
- **Validated on load:** the worker checks every field it uses (`worker/catalogue.ts`); a malformed file fails the
  load or reload and names the file

## Generator Options

```bash
cargo run -p green-rosetta-generator --release -- generate \
  --output-dir <DIR>         # Output root directory (creates <DIR>/media and <DIR>/metadata)
  [DIR]...                   # Album dirs (default) OR scan dirs (with --scan)
  --scan                     # Treat each DIR as a scan directory (scan direct subdirs)
  --filter <REGEX>           # Optional: only with --scan; matches artist/album after scanning tags
```

**Example:**

```bash
# Process all albums
cargo run -p green-rosetta-generator --release -- generate \
  --output-dir ./out \
  --scan /tank/mus/bandcamp

# Process only Vocaloid albums
cargo run -p green-rosetta-generator --release -- generate \
  --output-dir ./out \
  --scan /tank/mus/bandcamp \
  --filter "miku|gumi|rin"
```

## Testing

```bash
pnpm test        # worker
cargo test       # generator
```

## Troubleshooting

### Every request fails with a metadata error

- The station loads `metadata/*.yaml` from the bucket when it starts; the error names the file and field
- Fix the file, upload it, and the next request retries

### No cover images or no audio

- Check that `PUBLIC_MEDIA_URL` serves `media/...` from the bucket: open a `cover_url` from `/api/albums/<id>`
- Check the bucket's CORS policy allows the player's origin
- Check browser console for DASH.js errors

### Frontend API errors

- Ensure `pnpm dev` is running on port 8787 (or set `VITE_API_URL`)
- Browser DevTools → Network tab to inspect requests

## Resources

- [API Documentation](./api.md)
- [DASH.js Documentation](https://dashjs.org/)
- [Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/)
- [Vite Build Tool](https://vitejs.dev/)
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
