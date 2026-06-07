use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod s3_loader;

/// Database structure matching per-album YAML metadata format.
#[derive(Debug, Clone)]
pub struct Database {
    pub albums: Vec<Album>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub release_date: Option<String>,
    pub original_release_date: Option<String>,
    pub media: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub musicbrainz_album_id: Option<String>,
    pub disc_count: u32,
    pub cover_path: String,
    /// Runtime-only flag controlled by the admin UI. Not persisted in YAML.
    #[serde(skip_serializing, default = "default_album_enabled")]
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub images: Vec<AlbumImage>,
    pub tracks: Vec<Track>,
}

fn default_album_enabled() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumImage {
    pub id: String,
    pub image_path: String,
    pub image_type: String,
    pub is_primary: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub disc_number: u32,
    pub track_number: u32,
    pub length_seconds: f64,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub representations: Vec<TrackRepresentation>,
    // Legacy single-representation fields (optional; kept for compatibility/migration).
    pub encoded_length_seconds: Option<f64>,
    pub init_segment_path: Option<String>,
    pub segment_path_template: Option<String>,
    pub segment_count: Option<u32>,
    pub segment_duration: Option<f64>,
    pub segment_timescale: Option<u32>,
    pub segment_timeline: Option<Vec<SegmentTimelineEntry>>,
    pub source_file_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SegmentTimelineEntry {
    pub t: Option<u64>,
    pub d: u64,
    pub r: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackRepresentation {
    pub name: String,
    #[serde(
        default,
        alias = "adaption_set_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub adaptation_set_id: Option<String>,
    pub codec: String,
    pub mpd_codecs: String,
    pub bandwidth: u32,
    pub sample_rate: u32,
    pub channels: u8,
    pub init_segment_path: String,
    pub segment_path_template: String,
    pub segment_count: u32,
    pub segment_timescale: Option<u32>,
    pub segment_timeline: Option<Vec<SegmentTimelineEntry>>,
    pub encoded_length_seconds: Option<f64>,
}

impl Track {
    pub fn effective_representations(&self) -> Vec<TrackRepresentation> {
        if !self.representations.is_empty() {
            return self.representations.clone();
        }

        // Legacy metadata: expose a single "default" representation.
        let (init, template, count) = match (
            self.init_segment_path.as_ref(),
            self.segment_path_template.as_ref(),
            self.segment_count,
        ) {
            (Some(i), Some(t), Some(c)) => (i.clone(), t.clone(), c),
            _ => return Vec::new(),
        };

        vec![TrackRepresentation {
            name: "default".to_string(),
            adaptation_set_id: None,
            codec: "aac".to_string(),
            mpd_codecs: "mp4a.40.2".to_string(),
            bandwidth: 128_000,
            sample_rate: 48_000,
            channels: 2,
            init_segment_path: init,
            segment_path_template: template,
            segment_count: count,
            segment_timescale: self.segment_timescale,
            segment_timeline: self.segment_timeline.clone(),
            encoded_length_seconds: self.encoded_length_seconds,
        }]
    }

    /// Duration derived from the encoded output timeline when available.
    /// Falls back to encoded_length_seconds, then to the segment_count * segment_duration estimate.
    pub fn encoded_duration_seconds(&self) -> f64 {
        // Prefer timeline-derived duration from a representation (matches MPD timing).
        //
        // When multiple AdaptationSets/codecs exist, choose a stable "timing reference"
        // representation so Period boundaries remain consistent for most clients.
        if !self.representations.is_empty() {
            let rep = self
                .representations
                .iter()
                .find(|r| r.mpd_codecs.starts_with("mp4a")) // prefer AAC-in-MP4
                .or_else(|| {
                    self.representations
                        .iter()
                        .find(|r| r.segment_timescale.is_some() && r.segment_timeline.is_some())
                })
                .unwrap_or(&self.representations[0]);

            if let (Some(timescale), Some(timeline)) =
                (rep.segment_timescale, &rep.segment_timeline)
                && timescale > 0
            {
                let mut total_ticks: u64 = 0;
                for entry in timeline {
                    let repeats = entry.r.unwrap_or(0);
                    if repeats < 0 {
                        continue;
                    }
                    let count = (repeats as u64) + 1;
                    total_ticks = total_ticks.saturating_add(entry.d.saturating_mul(count));
                }
                return total_ticks as f64 / timescale as f64;
            }
            if let Some(encoded) = rep.encoded_length_seconds {
                return encoded;
            }
            return rep.segment_count as f64 * self.segment_duration.unwrap_or(6.0);
        }

        // Legacy fields.
        if let (Some(timescale), Some(timeline)) = (self.segment_timescale, &self.segment_timeline)
            && timescale > 0
        {
            let mut total_ticks: u64 = 0;
            for entry in timeline {
                let repeats = entry.r.unwrap_or(0);
                if repeats < 0 {
                    continue;
                }
                let count = (repeats as u64) + 1;
                total_ticks = total_ticks.saturating_add(entry.d.saturating_mul(count));
            }
            return total_ticks as f64 / timescale as f64;
        }

        if let Some(encoded) = self.encoded_length_seconds {
            return encoded;
        }

        self.segment_duration.unwrap_or(6.0) * self.segment_count.unwrap_or(0) as f64
    }
}

impl Database {
    /// Load metadata from a directory containing one YAML file per album.
    pub fn load(metadata_dir: &Path) -> Result<Self> {
        if !metadata_dir.exists() {
            return Err(anyhow!(
                "Metadata directory not found at {}",
                metadata_dir.display()
            ));
        }
        if !metadata_dir.is_dir() {
            return Err(anyhow!(
                "Metadata path is not a directory: {}",
                metadata_dir.display()
            ));
        }

        let mut albums = Vec::new();
        for entry in std::fs::read_dir(metadata_dir)
            .with_context(|| format!("Failed to read metadata dir {}", metadata_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let mut album: Album = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse album YAML {}", path.display()))?;
            // Ignore any persisted value and treat enabled as runtime-only.
            album.enabled = true;
            albums.push(album);
        }

        Ok(Database { albums })
    }

    /// Carry over runtime-only `enabled` flags from a previous database, matched by album id.
    ///
    /// `enabled` is admin-controlled and not persisted in YAML (a freshly loaded `Database` has
    /// every album enabled), so a reload must re-apply the previous toggles. Albums present only
    /// in the old DB are dropped; albums new in this DB keep their default (enabled).
    pub fn inherit_enabled_from(&mut self, old: &Database) {
        use std::collections::HashMap;
        let prev: HashMap<&str, bool> = old
            .albums
            .iter()
            .map(|a| (a.id.as_str(), a.enabled))
            .collect();
        for a in &mut self.albums {
            if let Some(&enabled) = prev.get(a.id.as_str()) {
                a.enabled = enabled;
            }
        }
    }

    /// Get album by ID
    pub fn get_album(&self, id: &str) -> Option<&Album> {
        self.albums.iter().find(|a| a.id == id)
    }

    /// Get all enabled albums
    pub fn get_enabled_albums(&self) -> Vec<&Album> {
        self.albums.iter().filter(|a| a.enabled).collect()
    }

    /// Whether a specific track of a specific album currently exists (cheap; no cloning).
    pub fn has_track(&self, album_id: &str, track_id: &str) -> bool {
        self.get_album(album_id)
            .is_some_and(|a| a.tracks.iter().any(|t| t.id == track_id))
    }

    /// Get track by ID (searches across all albums)
    pub fn get_track(&self, track_id: &str) -> Option<(&Album, &Track)> {
        for album in &self.albums {
            if let Some(track) = album.tracks.iter().find(|t| t.id == track_id) {
                return Some((album, track));
            }
        }
        None
    }

    /// Get total number of tracks
    pub fn total_tracks(&self) -> usize {
        self.albums.iter().map(|a| a.tracks.len()).sum()
    }
}
