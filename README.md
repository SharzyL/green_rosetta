# Green Rosetta

A music radio streaming service with DASH/CMAF streaming for seamless, synchronized playback across multiple clients.

## Architecture

Green Rosetta consists of three components:

1. **Worker** (TypeScript, Cloudflare Workers): generates the live DASH manifest, keeps the station schedule, serves the API and the web player
2. **Generator** (Rust): CLI tool for processing audio files into CMAF segments and per-album metadata
3. **Frontend** (React/TypeScript): web player with live synchronization, plus the admin UI at `/scrutinizer/`

Media and metadata live in an R2 bucket. Players fetch segments straight from the bucket's public domain; the
worker only reads `metadata/*.yaml`. The station's schedule, album toggles and admin sessions are held by a
single Durable Object, so every listener hears the same thing and an eviction resumes rather than resets.

## Quick Start

### 1. Generate Audio Content

The generator is packaged by the Nix flake (or build it with `cargo build --release`). It needs FFmpeg with AAC
and Opus support.

```bash
nix build   # ./result/bin/green-rosetta-generator
```

Process your music library into CMAF segments:

```bash
green-rosetta-generator generate \
  --scan /path/to/music \
  --output-dir ./data \
  --filter "Artist Name" \
  --jobs 4
```

Note that currently we only support scanning a flat-layout directory. It means that every subdirectory to /path/to/music represents an album (possibly with multiple discs). Each album sholud contain a `{cover,foler}.{jpg,png}` in its root. And it is highly suggested to tag all audio files with [Picard](https://picard.musicbrainz.org/).

This creates:

- `./data/media/` - CMAF segments and cover images
- `./data/metadata/` - Per-album YAML metadata files

### 2. Upload to R2

The bucket layout mirrors `./data`: `media/` and `metadata/` at the root. Give the bucket a public domain
(or custom domain) so players can fetch `media/` directly, and allow the player's origin in the bucket's CORS
policy.

[rclone](https://rclone.org/) is the recommended way to upload. Configure an S3 remote once with
`rclone config` (type `s3`, provider `Cloudflare`, your R2 access key, and
`https://<account_id>.r2.cloudflarestorage.com` as the endpoint); the entry in `~/.config/rclone/rclone.conf`
looks like:

```ini
[r2]
type = s3
provider = Cloudflare
access_key_id = YOUR_ACCESS_KEY
secret_access_key = YOUR_SECRET_KEY
endpoint = https://<account_id>.r2.cloudflarestorage.com
```

Then sync the generator output into the bucket:

```bash
rclone sync ./data r2:rosetta --progress
```

Use `rclone sync ./data/media r2:rosetta/media` to push only the (much larger) media segments when metadata
has not changed. Add `--checksum` to skip already-uploaded segments without re-reading timestamps, and
`--transfers 16` to parallelize uploads.

After uploading new metadata, press "reload" in the admin UI (or `POST /api/admin/reload`) so the running
station picks it up.

### 3. Deploy the Worker

Edit the bottom part of `wrangler.toml`: the R2 `bucket_name`, `PUBLIC_MEDIA_URL` (the bucket's public
origin), and optionally a `[[routes]]` custom domain. Then set the admin credentials and deploy:

```bash
pnpm install
pnpm wrangler secret put SCRUTINIZER_USERNAME
pnpm -s hash-password | pnpm wrangler secret put SCRUTINIZER_PASSWORD_HASH
pnpm build:frontend
pnpm deploy
```

`hash-password` uses 8 bcrypt rounds by default (`pnpm hash-password -n 10` for more); verification runs
inside the Worker, so keep the cost low.

Pushes to `goshujin` deploy automatically through `.github/workflows/deploy.yml` once the repository has
`CF_API_TOKEN` and `CF_ACCOUNT_ID` secrets in a `deploy` environment.

## Configuration

### Worker (`wrangler.toml`)

- `PUBLIC_MEDIA_URL` - public origin of the bucket; segment URLs are `{PUBLIC_MEDIA_URL}/media/...`
- `TIME_SHIFT_BUFFER_DEPTH` - DVR window in seconds
- `SUGGESTED_PRESENTATION_DELAY` - client startup delay behind the live edge
- `MIN_FUTURE_MANIFEST_DURATION` - how much timeline ahead of now the manifest always covers
- `ADMIN_RATE_LIMIT_PER_SECOND` - requests per second across all admin endpoints
- `ADMIN_SESSION_TTL_SECONDS` - admin cookie lifetime
- Secrets: `SCRUTINIZER_USERNAME`, `SCRUTINIZER_PASSWORD_HASH` (bcrypt)

The worker does not define audio profiles. Audio encoding profiles / adaptation sets are configured in
`generator.config.toml` and persisted into `metadata/*.yaml` by the generator.

### Generator Configuration

Create `generator.config.toml` (gitignored; start from `docs/generator.config.toml`):

```toml
[streaming]
segment_duration = 6.0

[[audio.adaption_sets]]
codec = "aac"
bitrates = ["96k", "128k", "192k"]
sample_rate = 48000
channels = 2
```

## API Reference

See [docs/api.md](docs/api.md).

### Streaming Endpoints

- `GET /stream/manifest.mpd` - DASH manifest (segments are fetched from `PUBLIC_MEDIA_URL`)

### Metadata Endpoints

- `GET /api/now?t={seconds}` - What is on air at a point on the presentation timeline
- `GET /api/tracks/{track_id}` - Track information
- `GET /api/albums/{album_id}` - Album information with tracklist
- `GET /api/sync/time` - Server time for synchronization
- `GET /api/utc` - UTC time in DASH format

### Admin Endpoints

- `POST /api/admin/login` - Create a cookie session
- `POST /api/admin/logout` - Clear the cookie session
- `GET /api/admin/me` - Session check
- `GET /api/admin/albums` - List all albums
- `PUT /api/admin/albums/{album_id}/enabled` - Enable/disable album
- `POST /api/admin/reload` - Re-read metadata from the bucket

Admin endpoints use a cookie session (`gr_admin_session`) and require configured credentials. Album toggles
persist across restarts.

## Development

```bash
pnpm install

pnpm test           # worker tests, in the Workers runtime
pnpm lint           # biome
pnpm typecheck
pnpm fmt

cargo test          # generator
cargo clippy
nix fmt
```

`pnpm dev` runs the worker locally on port 8787 against the real bucket (`remote = true` in `wrangler.toml`;
the worker never writes to it) and serves the last `pnpm build:frontend`. Put local admin credentials in
`.dev.vars`:

```ini
SCRUTINIZER_USERNAME=admin
SCRUTINIZER_PASSWORD_HASH=$2b$08$...
```

For frontend work with hot reload, run `pnpm dev` and `pnpm dev:frontend` side by side; Vite (port 3000)
proxies `/api` and `/stream` to the worker.
