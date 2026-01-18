# Green Rosetta

A music radio streaming service with DASH/CMAF streaming for seamless, synchronized playback across multiple clients.

## Architecture

Green Rosetta consists of three components:

1. **Backend** (Rust/Axum): Streaming server that generates DASH manifests and serves media
2. **Generator** (Rust): CLI tool for processing audio files into CMAF segments
3. **Frontend** (React/TypeScript): Web player with live synchronization

## Installation

### Using Nix (Recommended)

Build and install the package:

```bash
nix build
```

The package installs:
- `/bin/green-rosetta-backend` - Streaming server
- `/bin/green-rosetta-generator` - Audio processing tool
- `/share/green-rosetta/www/` - Web player assets

### NixOS Module

Add to your NixOS configuration:

```nix
{
  inputs.green-rosetta.url = "github:SharzyL/green-rosetta";

  outputs = { nixpkgs, green-rosetta, ... }: {
    nixosConfigurations.yourhost = nixpkgs.lib.nixosSystem {
      modules = [
        green-rosetta.nixosModules.default
        {
          services.green-rosetta = {
            enable = true;
            listen = "0.0.0.0:8080";
            configFile = "/etc/green-rosetta/backend.config.toml";
          };
        }
      ];
    };
  };
}
```

### Manual Installation

Requirements:
- Rust 1.70+
- Node.js 18+
- pnpm
- FFmpeg with AAC support

```bash
# Build backend and generator
cargo build --release --workspace

# Build frontend
cd frontend
pnpm install
pnpm build
```

## Quick Start

### 1. Generate Audio Content

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

### 2. Configure Backend

Create `backend.config.toml` and `generator.config.toml`.

Both files are gitignored. Start from the examples in `docs/`:

```bash
cp docs/backend.config.toml ./backend.config.toml
cp docs/generator.config.toml ./generator.config.toml
```

Minimal `backend.config.toml` (local storage) looks like:

```toml
[storage]
backend = "local"
base_path = "./data"

[streaming]
time_shift_buffer_depth = 30.0
suggested_presentation_delay = 10.0
min_future_manifest_duration = 60.0

[admin]
username = "admin"
password_hash = "$2b$12$REPLACE_ME_WITH_A_BCRYPT_HASH"

[logging]
level = "info"
```

### 3. Start Backend

```bash
pnpm -C frontend build
green-rosetta-backend --config backend.config.toml --web-dir frontend/dist
```

The server will:
- Load album metadata from `./data/metadata/`
- Serve DASH manifests at `/stream/manifest.mpd`
- Serve media segments from `./data/media/`
- Provide REST API at `/api/*`

### 4. Access Web Player

Open your browser to `http://localhost:8080` to access the web player.

## Configuration

### Backend Configuration

**Server Settings:**
- `server.host` - Listen address (default: "0.0.0.0")
- `server.port` - Listen port (default: 8080)
- `server.base_url` - Public URL for manifest generation

**Storage Settings:**
- `storage.backend` - "local" or "s3"
- `storage.base_path` - Root directory for local storage
- `storage.s3.*` - S3 configuration (bucket, region, credentials)

**Streaming Settings:**
- `streaming.time_shift_buffer_depth` - DVR window in seconds
- `streaming.suggested_presentation_delay` - Client startup delay
- `streaming.min_future_manifest_duration` - Future timeline coverage

Backend does not define audio profiles. Audio encoding profiles / adaptation sets are configured
in `generator.config.toml` and persisted into `metadata/*.yaml` by the generator.

### Generator Configuration

Create `generator.config.toml`:

```toml
[streaming]
segment_duration = 6.0

[[audio.adaption_sets]]
codec = "aac"
bitrates = ["96k", "128k", "192k"]
sample_rate = 48000
channels = 2
```

### S3 Storage

For S3-compatible storage:

```toml
[storage]
backend = "s3"

[storage.s3]
bucket = "my-music-bucket"
region = "us-east-1"
endpoint = "https://s3.amazonaws.com"
access_key_id = "YOUR_ACCESS_KEY"
public_access_domain = "https://cdn.example.com"
```

Set the secret key via environment variable:

```bash
export AWS_SECRET_ACCESS_KEY="your-secret-key"
```

## API Reference

### Streaming Endpoints

- `GET /stream/manifest.mpd` - DASH manifest
- `GET /media/albums/{album}/track_{disc}_{track}/{profile}/init.mp4` - Init segment
- `GET /media/albums/{album}/track_{disc}_{track}/{profile}/chunk_{N}.m4s` - Media segment

### Metadata Endpoints

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

Admin endpoints use a cookie session (`gr_admin_session`) and require configured credentials.

## Development

```bash
# Run Rust tests
cargo test --workspace

# Format code
cargo fmt --all
nix fmt

# Lint
cargo clippy --workspace
pnpm -C frontend check
```

For frontend development, run the backend without `--web-dir` option.

```bash
green-rosetta-backend --config backend.config.toml &
cd frontend
pnpm dev
```

Note that in this case the frontend expects backend at `http://localhost:8081`, as configured in `vite.config.ts`.

## Deployment

### Systemd Service

Create `/etc/systemd/system/green-rosetta.service`:

```ini
[Unit]
Description=Green Rosetta Music Streaming Service
After=network.target

[Service]
Type=simple
User=green-rosetta
Group=green-rosetta
ExecStart=/usr/local/bin/green-rosetta-backend --config /etc/green-rosetta/backend.config.toml
Restart=on-failure
RestartSec=5s

# Security hardening
DynamicUser=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl enable --now green-rosetta
```

