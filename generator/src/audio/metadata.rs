use anyhow::{Result, anyhow};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Extract metadata from audio file using ffprobe
pub fn extract_metadata(path: &Path) -> Result<AudioMetadata> {
    // Run ffprobe to get JSON output
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path.to_str().ok_or_else(|| anyhow!("Invalid file path"))?,
        ])
        .output()
        .map_err(|e| anyhow!("Failed to run ffprobe on {}: {}", path.display(), e))?;

    if !output.status.success() {
        return Err(anyhow!(
            "ffprobe failed on {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let json_str = String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("Invalid UTF-8 in ffprobe output: {}", e))?;

    let json: Value = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse ffprobe JSON output: {}", e))?;

    // Extract duration from first audio stream or format metadata
    let duration = json["streams"]
        .as_array()
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream["codec_type"].as_str() == Some("audio"))
                .and_then(|stream| stream["duration"].as_str())
                .and_then(|d| d.parse::<f64>().ok())
        })
        .or_else(|| {
            json["format"]["duration"]
                .as_str()
                .and_then(|d| d.parse::<f64>().ok())
        })
        .ok_or_else(|| {
            anyhow!(
                "Unable to extract duration from {} - no audio stream or format duration found",
                path.display()
            )
        })?;

    if duration <= 0.0 {
        return Err(anyhow!(
            "Invalid duration {} in {}",
            duration,
            path.display()
        ));
    }

    // Extract metadata from format tags
    let tags = &json["format"]["tags"];

    // Helper function to get tag value (case-insensitive)
    let get_tag = |key_lower: &str| -> Option<String> {
        // Try uppercase first (most common)
        if let Some(val) = tags[key_lower.to_uppercase()].as_str() {
            return Some(val.to_string());
        }
        // Try original case
        if let Some(val) = tags[key_lower].as_str() {
            return Some(val.to_string());
        }
        None
    };

    let get_tag_any = |keys: &[&str]| -> Option<String> {
        for k in keys {
            if let Some(v) = get_tag(k) {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
        None
    };

    // Title is required
    let title = get_tag("title")
        .ok_or_else(|| anyhow!("Missing required 'title' tag in {}", path.display()))?;

    // Artist is required
    let artist = get_tag("artist")
        .ok_or_else(|| anyhow!("Missing required 'artist' tag in {}", path.display()))?;

    // Album is required
    let album = get_tag("album")
        .ok_or_else(|| anyhow!("Missing required 'album' tag in {}", path.display()))?;

    // Album artist (optional; fall back handled by caller)
    // Track number is required
    let track_number = get_tag("track")
        .ok_or_else(|| anyhow!("Missing required 'track' tag in {}", path.display()))?
        .parse::<u32>()
        .map_err(|_| anyhow!("Invalid track number in {}", path.display()))?;

    // Disc number (optional, defaults to 1 with warning)
    let disc_number = get_tag("disc")
        .and_then(|d| d.parse::<u32>().ok())
        .unwrap_or_else(|| {
            tracing::warn!("Missing disc number in {}, defaulting to 1", path.display());
            1
        });

    // Disc total (optional, defaults to 1 with warning)
    let disc_total = get_tag("TOTALDISCS")
        .and_then(|d| d.parse::<u32>().ok())
        .unwrap_or_else(|| {
            tracing::warn!("Missing TOTALDISCS in {}, defaulting to 1", path.display());
            1
        });

    // Optional tags (best-effort; varies by tagging tools)
    // Keep DATE and ORIGINALDATE distinct so the UI can display both when they differ.
    let date = get_tag_any(&["date", "year"]);
    let original_date = get_tag_any(&["originaldate", "original_date"]);
    let label = get_tag_any(&["label", "publisher", "organization"]);
    let media = get_tag_any(&["media"]);
    let catalog_number = get_tag_any(&["catalognumber", "catalog_number", "catalog"]);
    let musicbrainz_album_id = get_tag_any(&["musicbrainz_albumid"]);

    Ok(AudioMetadata {
        title,
        artist,
        album,
        track_number,
        disc_number,
        disc_total,
        duration,
        date,
        original_date,
        label,
        media,
        catalog_number,
        musicbrainz_album_id,
    })
}

#[derive(Debug, Clone)]
pub struct AudioMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: u32,
    pub disc_number: u32,
    pub disc_total: u32,
    pub duration: f64,
    pub date: Option<String>,
    pub original_date: Option<String>,
    pub label: Option<String>,
    pub media: Option<String>,
    pub catalog_number: Option<String>,
    pub musicbrainz_album_id: Option<String>,
}
