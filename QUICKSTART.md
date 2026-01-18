# Green Rosetta - Quick Start Guide

A music radio service with DASH/CMAF streaming for seamless playback.

## 🚀 Quick Test Setup

### 1. Generate Test Data

Process the Kikuo album from your collection:

```bash
cd /home/sharzy/ws/dev/proj/green_rosetta

# Generate CMAF chunks and metadata (takes ~2-3 minutes)
cargo run -p green-rosetta-generator -- generate \
  --output-dir ./test_dir \
  --scan /tank/mus/bandcamp \
  --filter Kikuo
```

### 2. Create Backend Config

Create `backend.config.toml` (backend) and `generator.config.toml` (generator) in the project root:

```toml
[server]
host = "0.0.0.0"
port = 8080
base_url = "http://localhost:8080"

[storage]
backend = "local"
base_path = "./test_dir"

[streaming]
time_shift_buffer_depth = 30.0
suggested_presentation_delay = 10.0

[[audio.profiles]]
name = "aac_128k"
codec = "aac"
bitrate = "128k"
sample_rate = 48000
channels = 2

[admin]
username = "admin"
password_hash = ""

[logging]
level = "info"
```

### 3. Start Backend Server

```bash
cargo run -p green-rosetta-backend
```

You should see:
```
2026-01-17T12:00:00Z  INFO green_rosetta_backend: Loaded 1 albums with 11 total tracks
2026-01-17T12:00:00Z  INFO green_rosetta_backend: Server listening on 0.0.0.0:8080
```

### 4. Test the API

In another terminal:

```bash
# Get current now-playing info
curl http://localhost:8080/api/now-playing | jq

# Get server time for sync
curl http://localhost:8080/api/sync/time | jq

# Get DASH manifest
curl http://localhost:8080/stream/manifest.mpd
```

### 5. Stream in Browser

**Option A: Simple HTML Player** (Recommended for testing)

Serve the simple HTML player:
```bash
# Start a simple HTTP server
cd frontend
python3 -m http.server 3000
```

Then open: `http://localhost:3000/index.html`

The player will:
- Load the DASH manifest from your backend
- Display current track information
- Show album artwork
- Display progress as you listen
- Auto-update every 2 seconds

**Option B: React Frontend**

```bash
cd frontend
npm install
npm start
```

Then open: `http://localhost:3000`

## 📋 Project Structure

```
green_rosetta/
├── backend/              # Rust Axum server
│   ├── src/
│   │   ├── main.rs      # Routes & handlers
│   │   ├── db/          # TOML database loading
│   │   ├── config.rs    # Configuration structures
│   │   ├── storage/     # Storage abstraction (local/S3)
│   │   ├── scheduler.rs # Playback state management
│   │   └── streaming/   # DASH MPD generation
│   └── Cargo.toml
│
├── generator/           # CLI tool for audio processing
│   ├── src/
│   │   ├── main.rs     # CLI entry point
│   │   ├── scan.rs     # Album discovery
│   │   ├── audio/      # FFmpeg encoding & metadata extraction
│   │   └── db/         # Metadata import
│   └── Cargo.toml
│
├── frontend/           # Web player
│   ├── index.html     # Standalone player (no build needed)
│   ├── src/           # React components (optional)
│   └── package.json
│
├── backend.config.toml   # Server configuration
├── generator.config.toml # Generator configuration
├── metadata/          # Music library metadata (one YAML file per album)
└── test_dir/          # Generated test data
    ├── media/         # CMAF chunks
    └── metadata/      # Test metadata
```

## 🎯 Architecture

### Backend Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /health` | Health check |
| `GET /stream/manifest.mpd` | DASH manifest |
| `GET /stream/init.mp4` | CMAF initialization segment |
| `GET /stream/chunk/:number` | Audio chunks |
| `GET /api/now-playing` | Current track info |
| `GET /api/sync/time` | Server time for sync |

### Streaming Flow

1. Client requests `/stream/manifest.mpd`
2. Backend generates MPD with current + 5 upcoming tracks
3. Client plays video via DASH.js
4. DASH.js requests init segment and chunks
5. Client periodically polls `/api/now-playing` to update UI

### Key Features

- ✅ **Seamless Playback**: Multi-period DASH MPD for <50ms gaps
- ✅ **Time Sync**: UTC-based synchronization across clients
- ✅ **CMAF Encoding**: FFmpeg generates CMAF chunks with 6-second segments
- ✅ **Flexible Storage**: Local filesystem (S3 support ready)
- ✅ **Strict Metadata**: FFprobe extraction with error on missing critical fields
- ✅ **Simple Database**: TOML file-based (humanly readable)

## 🔧 Troubleshooting

### Player shows "Loading..." forever
- Check backend is running: `curl http://localhost:8080/health`
- Check `metadata/` exists and contains `*.yaml` files
- Check manifest generation: `curl http://localhost:8080/stream/manifest.mpd`

### 404 errors on chunks
- Ensure CMAF files were generated in `./test_dir/media/albums/...`
- Check `storage.local.base_path` in `backend.config.toml` matches

### FFmpeg errors during generation
- Ensure FFmpeg and FFprobe are installed: `ffmpeg -version`
- Check audio files have metadata tags (title, artist, album, track)

## 📚 Next Steps

- Add parallel processing for generate encoding (currently sequential)
- Implement admin endpoints for enable/disable albums
- Add authentication for admin endpoints
- Implement S3 storage backend
- Add support for multiple audio profiles (adaptive bitrate)
- Create advanced React UI with queue management
