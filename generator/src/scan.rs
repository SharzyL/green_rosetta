use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::audio::metadata::extract_metadata;

/// Album information discovered during scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumInfo {
    pub name: String,
    pub artist: String,
    /// DATE tag (e.g. release date / reissue date).
    pub release_date: Option<String>,
    /// ORIGINALDATE tag (e.g. original release date).
    pub original_release_date: Option<String>,
    pub label: Option<String>,
    pub media: Option<String>,
    pub catalog_number: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub cover_path: PathBuf,
    pub tracks: Vec<TrackInfo>,
}

/// Track information discovered during scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub path: PathBuf,
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

pub struct Scanner;

impl Scanner {
    pub fn new() -> Self {
        Scanner
    }

    /// Scan a single album directory
    pub fn scan_album(&self, path: &Path) -> Result<AlbumInfo> {
        if !path.is_dir() {
            return Err(anyhow!("Path is not a directory: {}", path.display()));
        }

        // Find cover image
        let cover_path = self.find_cover(path)?;

        // Find and scan audio tracks
        let tracks = self.find_tracks(path)?;

        if tracks.is_empty() {
            return Err(anyhow!("No audio tracks found in {}", path.display()));
        }

        // Use embedded tags as source-of-truth (do not depend on directory name).
        let album_name = tracks[0].album.trim().to_string();
        if album_name.is_empty() {
            return Err(anyhow!(
                "Empty ALBUM tag in first track: {}",
                tracks[0].path.display()
            ));
        }

        let artist = tracks[0].artist.clone();
        let release_date = tracks.iter().find_map(|t| t.date.clone());
        let original_release_date = tracks.iter().find_map(|t| t.original_date.clone());
        let label = tracks.iter().find_map(|t| t.label.clone());
        let media = tracks.iter().find_map(|t| t.media.clone());
        let catalog_number = tracks.iter().find_map(|t| t.catalog_number.clone());
        let musicbrainz_album_id = tracks.iter().find_map(|t| t.musicbrainz_album_id.clone());

        Ok(AlbumInfo {
            name: album_name,
            artist,
            release_date,
            original_release_date,
            label,
            media,
            catalog_number,
            musicbrainz_album_id,
            cover_path,
            tracks,
        })
    }

    /// Scan a directory containing multiple albums
    pub fn scan_directory(&self, path: &Path) -> Result<Vec<AlbumInfo>> {
        if !path.is_dir() {
            return Err(anyhow!("Path is not a directory: {}", path.display()));
        }

        let mut albums = Vec::new();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();

            if entry_path.is_dir() {
                match self.scan_album(&entry_path) {
                    Ok(album) => albums.push(album),
                    Err(e) => {
                        tracing::warn!("Failed to scan {}: {}", entry_path.display(), e);
                    }
                }
            }
        }

        if albums.is_empty() {
            return Err(anyhow!("No valid albums found in {}", path.display()));
        }

        albums.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(albums)
    }

    /// Find cover image (cover.jpg, cover.png, front.jpg, front.png)
    fn find_cover(&self, path: &Path) -> Result<PathBuf> {
        let candidates = ["cover.jpg", "cover.png", "front.jpg", "front.png"];

        for candidate in &candidates {
            let cover_path = path.join(candidate);
            if cover_path.exists() {
                return Ok(cover_path);
            }
        }

        Err(anyhow!("No cover image found in {}", path.display()))
    }

    /// Find all audio tracks in directory
    fn find_tracks(&self, path: &Path) -> Result<Vec<TrackInfo>> {
        let mut tracks = Vec::new();

        // Look for audio files directly in the album directory
        for entry in WalkDir::new(path)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();

            if let Some(ext) = entry_path.extension()
                && let Some(ext_str) = ext.to_str()
                && matches!(
                    ext_str.to_lowercase().as_str(),
                    "flac" | "mp3" | "m4a" | "opus" | "aac"
                )
            {
                match extract_metadata(entry_path) {
                    Ok(metadata) => {
                        tracks.push(TrackInfo {
                            path: entry_path.to_path_buf(),
                            title: metadata.title,
                            artist: metadata.artist,
                            album: metadata.album,
                            track_number: metadata.track_number,
                            disc_number: metadata.disc_number,
                            disc_total: metadata.disc_total,
                            duration: metadata.duration,
                            date: metadata.date,
                            original_date: metadata.original_date,
                            label: metadata.label,
                            media: metadata.media,
                            catalog_number: metadata.catalog_number,
                            musicbrainz_album_id: metadata.musicbrainz_album_id,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to extract metadata from {}: {}",
                            entry_path.display(),
                            e
                        );
                    }
                }
            }
        }

        // Ensure deterministic album ordering: disc first, then track number.
        tracks.sort_by_key(|t| (t.disc_number, t.track_number));
        Ok(tracks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_sorting_key() {
        // Ensure the ordering logic stays disc -> track.
        let mut tracks = [
            TrackInfo {
                path: PathBuf::from("b"),
                title: "t2".to_string(),
                artist: "a".to_string(),
                album: "al".to_string(),
                track_number: 2,
                disc_number: 1,
                disc_total: 1,
                duration: 1.0,
                date: None,
                original_date: None,
                label: None,
                media: None,
                catalog_number: None,
                musicbrainz_album_id: None,
            },
            TrackInfo {
                path: PathBuf::from("a"),
                title: "t1".to_string(),
                artist: "a".to_string(),
                album: "al".to_string(),
                track_number: 1,
                disc_number: 2,
                disc_total: 2,
                duration: 1.0,
                date: None,
                original_date: None,
                label: None,
                media: None,
                catalog_number: None,
                musicbrainz_album_id: None,
            },
            TrackInfo {
                path: PathBuf::from("c"),
                title: "t0".to_string(),
                artist: "a".to_string(),
                album: "al".to_string(),
                track_number: 1,
                disc_number: 1,
                disc_total: 2,
                duration: 1.0,
                date: None,
                original_date: None,
                label: None,
                media: None,
                catalog_number: None,
                musicbrainz_album_id: None,
            },
        ];

        tracks.sort_by_key(|t| (t.disc_number, t.track_number));
        assert_eq!((tracks[0].disc_number, tracks[0].track_number), (1, 1));
        assert_eq!((tracks[1].disc_number, tracks[1].track_number), (1, 2));
        assert_eq!((tracks[2].disc_number, tracks[2].track_number), (2, 1));
    }
}
