# Green Rosetta - Getting Started Guide

## Prerequisites

- Rust 1.70+
- Node.js 18+ and pnpm
- FFmpeg 5.0+ (for audio encoding)
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

### 2. Create Backend Configuration

Create `backend.config.toml` and `generator.config.toml`.

Both files are gitignored; start from the documented examples:

```bash
cp docs/backend.config.toml ./backend.config.toml
cp docs/generator.config.toml ./generator.config.toml
```

`backend.config.toml`:

```toml
[server]
host = "127.0.0.1"
port = 8080
base_url = "http://127.0.0.1:8080"

[storage]
backend = "local"
base_path = "./test_dir"

[streaming]
time_shift_buffer_depth = 30.0
suggested_presentation_delay = 10.0

[admin]
username = "admin"
password_hash = ""

[logging]
level = "info"
```

`generator.config.toml`:

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

### 3. Start Backend Server

```bash
# In one terminal
cargo run -p green-rosetta-backend --release -- \
  --config ./backend.config.toml \
  --reuse-addr
```

Server will start on http://127.0.0.1:8080

### 4. Test Backend API

```bash
# Check health
curl http://localhost:8080/health

# Get current track info
curl http://localhost:8080/api/now-playing | jq .

# Get server time
curl http://localhost:8080/api/sync/time | jq .

# Get DASH manifest
curl http://localhost:8080/stream/manifest.mpd

# Get first segment
curl http://localhost:8080/stream/init.mp4 > init.mp4
curl http://localhost:8080/stream/chunk/0 > chunk_0.m4s
```

### 5. Start Frontend

```bash
# In another terminal
cd frontend
pnpm install  # First time only
pnpm dev
```

Frontend will open at http://localhost:3000

## Project Structure

```
green_rosetta/
├── backend/                 # Rust backend (Axum, DASH streaming)
│   ├── src/
│   │   ├── main.rs         # Server entry point
│   │   ├── config.rs       # Configuration loading
│   │   ├── db/             # Database module (TOML parsing)
│   │   ├── scheduler.rs    # Playback scheduling
│   │   ├── storage/        # Storage abstraction (local/S3)
│   │   └── streaming/      # DASH/CMAF generation
│   ├── tests/              # Integration tests
│   └── Cargo.toml
├── generator/              # Rust metadata generator
│   ├── src/
│   │   ├── main.rs         # CLI entry point
│   │   ├── scan.rs         # Album directory scanning
│   │   ├── audio/
│   │   │   ├── metadata.rs # FFprobe metadata extraction
│   │   │   └── processor.rs # FFmpeg CMAF encoding
│   │   └── db/
│   │       └── importer.rs # Metadata TOML export
│   └── Cargo.toml
├── frontend/               # React + Vite frontend
│   ├── src/
│   │   ├── App.jsx         # Main app component
│   │   ├── index.js        # Entry point
│   │   └── components/
│   │       ├── Player.jsx  # DASH.js player
│   │       └── NowPlaying.jsx # Track info display
│   ├── vite.config.js      # Vite configuration
│   ├── index.html          # Root HTML
│   └── package.json
├── docs/
│   ├── api.md              # API documentation
│   └── getting-started.md  # This file
├── test_dir/
│   ├── metadata/           # Generated metadata (one YAML file per album)
│   └── media/              # Generated CMAF segments
├── backend.config.toml     # Backend configuration
├── generator.config.toml   # Generator configuration
└── Cargo.toml              # Workspace root
```

## Metadata Format

Metadata is stored as one YAML file per album under the metadata directory:

```toml
[[albums]]
id = "35f03850c0"
name = "Kikuo Miku 7"
artist = "Kikuo"
release_date = "2023-03-24"
disc_count = 1
cover_path = "albums/35f03850c0 - Kikuo - Kikuo_Miku_7/cover.jpg"
enabled = true
created_at = "2026-01-17T07:50:40.028188356+00:00"
updated_at = "2026-01-17T07:50:40.028188356+00:00"

[[albums.images]]
id = "181c0213e9"
image_path = "albums/35f03850c0 - Kikuo - Kikuo_Miku_7/cover.jpg"
image_type = "cover"
is_primary = true

[[albums.tracks]]
id = "662b7d4cd5"
title = "ふたつの木馬 / Twins at the carousel"
artist = "Kikuo"
disc_number = 1
track_number = 1
length_seconds = 39.96
init_segment_path = "albums/35f03850c0 - Kikuo - Kikuo_Miku_7/disc_1/track_1/init.mp4"
segment_path_template = "albums/35f03850c0 - Kikuo - Kikuo_Miku_7/disc_1/track_1/chunk_%03d$.m4s"
segment_count = 7
segment_duration = 6.0
source_file_path = "/tank/mus/bandcamp/Kikuo - [2023-03-24] Kikuo Miku 7/01 - ふたつの木馬.flac"
created_at = "2026-01-17T07:50:40.028188356+00:00"
```

**Key Features:**
- **10-digit hex IDs:** Collision-free unique identifiers
- **Multi-disc support:** `disc_number` prevents path collisions
- **CMAF segments:** 6-second audio chunks with template substitution
- **Path structure:** `albums/{id} - {artist} - {album}/disc_{n}/track_{m}/`

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
# Run all tests (backend + generator)
cargo test --tests

# Run specific test file
cargo test --test api_tests
cargo test --test api_integration

# Expected: 14 tests passed
```

## Troubleshooting

### Backend fails to start
- Check config file paths and syntax (`backend.config.toml` / `generator.config.toml`)
- Verify metadata file exists at configured path
- Try with `--reuse-addr` flag if port is in use

### No cover images
- Generator extracts cover images from album directories
- Cover image must be in main album folder (not in track folders)
- Verify `base_path` in config points to correct media directory

### Audio not playing
- Check DASH manifest: `curl http://localhost:8080/stream/manifest.mpd`
- Verify segment files exist in media directory
- Check browser console for DASH.js errors
- Try manual segment download: `curl http://localhost:8080/stream/chunk/0`

### Frontend API errors
- Ensure backend is running on configured port
- Check proxy configuration in `frontend/vite.config.js`
- Browser DevTools → Network tab to inspect requests

## Performance Tips

1. **Build in release mode:** Always use `--release` flag
2. **Use SSD:** Metadata and segment I/O benefits from fast storage
3. **Adjust buffer times:** Modify `streaming` config for network conditions
4. **Monitor logs:** Check tracing output for bottlenecks

## Next Steps

1. **Add more albums:** Run generator on different directories
2. **Customize frontend:** Edit React components in `frontend/src/`
3. **Implement authentication:** Add user system (config has `admin` section)
4. **Setup S3 storage:** Change storage backend in `backend.config.toml`
5. **Deploy:** Build frontend with `pnpm build`, run backend in container

## Resources

- [API Documentation](./api.md)
- [DASH.js Documentation](https://dashjs.org/)
- [Axum Web Framework](https://github.com/tokio-rs/axum)
- [Vite Build Tool](https://vitejs.dev/)
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
