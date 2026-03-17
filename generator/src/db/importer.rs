use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::{AudioProfile, bitrate_to_bps, mpd_codecs_for_profile};
use crate::scan::AlbumInfo;

/// Generate a random 10-digit hex string
pub fn generate_hex_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hash, Hasher};

    let mut hasher = RandomState::new().build_hasher();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // Use pointer address as additional entropy
    let ptr = &timestamp as *const _ as u64;

    timestamp.hash(&mut hasher);
    ptr.hash(&mut hasher);

    let hash = hasher.finish();
    format!("{:010x}", hash & 0xFFFFFFFFFF)
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
    pub created_at: String,
    pub updated_at: String,
    pub images: Vec<AlbumImage>,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumImage {
    pub id: String,
    pub image_path: String,
    pub image_type: String,
    pub is_primary: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SegmentTimelineEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<u64>,
    pub d: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_timescale: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_timeline: Option<Vec<SegmentTimelineEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoded_length_seconds: Option<f64>,
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
    // Legacy single-representation fields (optional; not written by the generator anymore).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub init_segment_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub segment_path_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub segment_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub segment_duration: Option<f64>,
    pub source_file_path: Option<String>,
    pub created_at: String,
}

fn parse_pt_duration_seconds(value: &str) -> Option<f64> {
    // Supports PT#H#M#S with optional fractional seconds, e.g. PT2M45.2S.
    let value = value.trim();
    let re = regex::Regex::new(r"^PT(?:(?P<h>\d+)H)?(?:(?P<m>\d+)M)?(?:(?P<s>\d+(?:\.\d+)?)S)?$")
        .ok()?;
    let caps = re.captures(value)?;
    let h = caps
        .name("h")
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(0.0);
    let m = caps
        .name("m")
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(0.0);
    let s = caps
        .name("s")
        .and_then(|m| m.as_str().parse::<f64>().ok())
        .unwrap_or(0.0);
    Some(h * 3600.0 + m * 60.0 + s)
}

fn parse_segment_timeline(mpd_xml: &str) -> Option<Vec<SegmentTimelineEntry>> {
    let start = mpd_xml.find("<SegmentTimeline")?;
    let timeline_start = mpd_xml[start..].find('>')? + start + 1;
    let end = mpd_xml[timeline_start..].find("</SegmentTimeline>")? + timeline_start;
    let timeline = &mpd_xml[timeline_start..end];

    let s_re = regex::Regex::new(r#"<S\s+([^/>]+?)/?>"#).ok()?;
    let attr_re = regex::Regex::new(r#"(\w+)="([^"]+)""#).ok()?;

    let mut entries = Vec::new();
    for caps in s_re.captures_iter(timeline) {
        let attrs = &caps[1];
        let mut t: Option<u64> = None;
        let mut d: Option<u64> = None;
        let mut r: Option<i64> = None;

        for acaps in attr_re.captures_iter(attrs) {
            let key = &acaps[1];
            let val = &acaps[2];
            match key {
                "t" => t = val.parse::<u64>().ok(),
                "d" => d = val.parse::<u64>().ok(),
                "r" => r = val.parse::<i64>().ok(),
                _ => {}
            }
        }

        let d = d?;
        entries.push(SegmentTimelineEntry { t, d, r });
    }

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn build_timeline_from_duration(
    timescale: u32,
    segment_duration_ticks: u64,
    total_seconds: f64,
) -> (Vec<SegmentTimelineEntry>, u32) {
    // Convert total duration to ticks; this can still be fractional at the string level, so round.
    let total_ticks = (total_seconds * timescale as f64).round().max(0.0) as u64;
    if segment_duration_ticks == 0 || total_ticks == 0 {
        return (
            vec![SegmentTimelineEntry {
                t: Some(0),
                d: total_ticks.max(1),
                r: None,
            }],
            1,
        );
    }

    let full = total_ticks / segment_duration_ticks;
    let rem = total_ticks % segment_duration_ticks;

    // If the track is shorter than a single segment.
    if full == 0 {
        return (
            vec![SegmentTimelineEntry {
                t: Some(0),
                d: total_ticks,
                r: None,
            }],
            1,
        );
    }

    let mut entries = Vec::new();
    if rem == 0 {
        entries.push(SegmentTimelineEntry {
            t: Some(0),
            d: segment_duration_ticks,
            r: Some(full as i64 - 1),
        });
        return (entries, full as u32);
    }

    // Full segments + one final shorter segment.
    entries.push(SegmentTimelineEntry {
        t: Some(0),
        d: segment_duration_ticks,
        r: Some(full as i64 - 1),
    });
    entries.push(SegmentTimelineEntry {
        t: None,
        d: rem,
        r: None,
    });
    (entries, (full + 1) as u32)
}

type TrackMpdInfo = (u32, Vec<SegmentTimelineEntry>, f64, u32);

fn parse_track_mpd(track_dir: &Path) -> Result<Option<TrackMpdInfo>> {
    // Returns (timescale, timeline_entries, encoded_length_seconds, segment_count)
    let mpd_path = track_dir.join("manifest.mpd");
    if !mpd_path.exists() {
        return Ok(None);
    }

    let mpd_xml = std::fs::read_to_string(&mpd_path)?;

    let timescale_re = regex::Regex::new(r#"timescale="(\d+)""#)?;
    let duration_re = regex::Regex::new(r#"duration="(\d+)""#)?;
    let mpd_dur_re = regex::Regex::new(r#"mediaPresentationDuration="([^"]+)""#)?;

    let timescale: u32 = timescale_re
        .captures(&mpd_xml)
        .and_then(|c| c.get(1))
        .ok_or_else(|| anyhow!("MPD missing SegmentTemplate timescale"))?
        .as_str()
        .parse()?;

    if let Some(entries) = parse_segment_timeline(&mpd_xml) {
        // Compute segment count from entries.
        let mut count: u32 = 0;
        let mut total_ticks: u64 = 0;
        for e in &entries {
            let r = e.r.unwrap_or(0);
            if r < 0 {
                return Err(anyhow!("Unsupported SegmentTimeline repeat r={}", r));
            }
            let entry_count = (r as u32) + 1;
            count = count.saturating_add(entry_count);
            total_ticks = total_ticks.saturating_add(e.d.saturating_mul(entry_count as u64));
        }
        let encoded_length_seconds = if timescale > 0 {
            total_ticks as f64 / timescale as f64
        } else {
            0.0
        };
        return Ok(Some((
            timescale,
            entries,
            encoded_length_seconds,
            count.max(1),
        )));
    }

    let encoded_length_seconds = mpd_dur_re
        .captures(&mpd_xml)
        .and_then(|c| c.get(1))
        .and_then(|m| parse_pt_duration_seconds(m.as_str()))
        .ok_or_else(|| anyhow!("MPD missing mediaPresentationDuration"))?;

    // Fall back to fixed-duration SegmentTemplate with total duration from mediaPresentationDuration.
    let segment_duration_ticks: u64 = duration_re
        .captures(&mpd_xml)
        .and_then(|c| c.get(1))
        .ok_or_else(|| anyhow!("MPD missing SegmentTemplate duration"))?
        .as_str()
        .parse()?;

    let (entries, count) =
        build_timeline_from_duration(timescale, segment_duration_ticks, encoded_length_seconds);
    Ok(Some((timescale, entries, encoded_length_seconds, count)))
}

fn sanitize_dir_component(s: &str) -> String {
    // Keep in sync with the generator's media folder naming.
    s.trim().replace(' ', "_")
}

fn album_dir_name(album_id: &str, artist: &str, album_name: &str) -> String {
    format!(
        "{} - {} - {}",
        album_id,
        sanitize_dir_component(artist),
        sanitize_dir_component(album_name)
    )
}

fn is_yaml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| matches!(s.to_ascii_lowercase().as_str(), "yaml" | "yml"))
        .unwrap_or(false)
}

fn load_albums_from_dir(metadata_dir: &Path) -> Result<Vec<Album>> {
    if !metadata_dir.exists() {
        return Ok(Vec::new());
    }
    if !metadata_dir.is_dir() {
        return Err(anyhow!(
            "metadata path is not a directory: {}",
            metadata_dir.display()
        ));
    }

    let mut albums = Vec::new();
    for entry in std::fs::read_dir(metadata_dir)
        .with_context(|| format!("Failed to read metadata dir {}", metadata_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_yaml_path(&path) {
            continue;
        }
        let content =
            std::fs::read_to_string(&path).with_context(|| format!("Read {}", path.display()))?;
        let album: Album = serde_yaml::from_str(&content)
            .with_context(|| format!("Parse album YAML {}", path.display()))?;
        albums.push(album);
    }
    Ok(albums)
}

fn find_album_yaml_by_id(
    metadata_dir: &Path,
    album_id: &str,
) -> Result<Option<std::path::PathBuf>> {
    if !metadata_dir.exists() {
        return Ok(None);
    }
    let mut found: Option<std::path::PathBuf> = None;
    for entry in std::fs::read_dir(metadata_dir)
        .with_context(|| format!("Failed to read metadata dir {}", metadata_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_yaml_path(&path) {
            continue;
        }
        let file = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f,
            None => continue,
        };
        if file.starts_with(&format!("{} - ", album_id)) {
            if found.is_some() {
                return Err(anyhow!(
                    "Multiple album metadata files found for album_id {} under {}",
                    album_id,
                    metadata_dir.display()
                ));
            }
            found = Some(path);
        }
    }
    Ok(found)
}

fn collect_used_ids(
    albums: &[Album],
) -> (
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
) {
    let mut album_ids = std::collections::HashSet::new();
    let mut track_ids = std::collections::HashSet::new();
    for a in albums {
        album_ids.insert(a.id.clone());
        for t in &a.tracks {
            track_ids.insert(t.id.clone());
        }
    }
    (album_ids, track_ids)
}

fn generate_unique_id(used: &std::collections::HashSet<String>) -> String {
    loop {
        let id = generate_hex_id();
        if !used.contains(&id) {
            return id;
        }
    }
}

/// Determine an album_id to use for an album import.
///
/// - If `provided` is Some, use it (and update any existing album with that id).
/// - Otherwise, if an album with matching (name, artist) exists, reuse its id.
/// - Otherwise, generate a new unique id.
pub fn determine_album_id(
    metadata_dir: &Path,
    album_info: &AlbumInfo,
    provided: Option<String>,
) -> Result<String> {
    std::fs::create_dir_all(metadata_dir)
        .with_context(|| format!("Create metadata dir {}", metadata_dir.display()))?;

    if let Some(id) = provided {
        return Ok(id);
    }

    let albums = load_albums_from_dir(metadata_dir)?;
    if let Some(existing) = albums
        .iter()
        .find(|a| a.name == album_info.name && a.artist == album_info.artist)
    {
        return Ok(existing.id.clone());
    }

    let (used_album_ids, _) = collect_used_ids(&albums);
    Ok(generate_unique_id(&used_album_ids))
}

/// Import album from AlbumInfo into per-album YAML under metadata_dir/.
pub async fn import_album(
    metadata_dir: &std::path::Path,
    output_base: &std::path::Path,
    profiles: &[AudioProfile],
    album_info: AlbumInfo,
    album_id: Option<String>,
) -> Result<()> {
    std::fs::create_dir_all(metadata_dir)
        .with_context(|| format!("Create metadata dir {}", metadata_dir.display()))?;

    let existing_albums = load_albums_from_dir(metadata_dir)?;
    let mut existing_album = existing_albums
        .iter()
        .find(|a| a.id == album_id.clone().unwrap_or_default())
        .cloned();

    // Determine album ID (if not provided, reuse by (name, artist) when possible).
    let album_id = match album_id {
        Some(id) => id,
        None => {
            if let Some(found) = existing_albums
                .iter()
                .find(|a| a.name == album_info.name && a.artist == album_info.artist)
            {
                existing_album = Some(found.clone());
                found.id.clone()
            } else {
                let (used_album_ids, _) = collect_used_ids(&existing_albums);
                generate_unique_id(&used_album_ids)
            }
        }
    };

    if existing_album.is_none() {
        existing_album = existing_albums.iter().find(|a| a.id == album_id).cloned();
    }

    // Get current timestamp
    let now = chrono::Utc::now().to_rfc3339();

    // Convert cover path to relative storage key
    let cover_filename = album_info
        .cover_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Invalid cover filename"))?;

    // Build album directory: albums/{album_id} - {artist} - {album_name}
    let artist_dir = album_info.artist.replace(" ", "_");
    let album_name = album_info.name.replace(" ", "_");
    let album_dir = format!("{} - {} - {}", album_id, artist_dir, album_name);
    let cover_path = format!("albums/{}/{}", album_dir, cover_filename);

    let (_, mut used_track_ids) = collect_used_ids(&existing_albums);

    // Reuse image id if we already have one, otherwise generate a new one.
    let image_id = existing_album
        .as_ref()
        .and_then(|a| a.images.first())
        .map(|img| img.id.clone())
        .unwrap_or_else(|| {
            let id = generate_unique_id(&used_track_ids);
            used_track_ids.insert(id.clone());
            id
        });

    // Create album images (single primary cover).
    let images = vec![AlbumImage {
        id: image_id,
        image_path: cover_path.clone(),
        image_type: "cover".to_string(),
        is_primary: true,
    }];

    // Create tracks
    let existing_tracks_by_pos: std::collections::BTreeMap<(u32, u32), Track> = existing_album
        .as_ref()
        .map(|a| {
            a.tracks
                .iter()
                .map(|t| ((t.disc_number, t.track_number), t.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut tracks = Vec::new();
    for track_info in album_info.tracks {
        let track_key = (track_info.disc_number, track_info.track_number);
        let existing_track = existing_tracks_by_pos.get(&track_key);
        let track_id = existing_track.map(|t| t.id.clone()).unwrap_or_else(|| {
            let id = generate_unique_id(&used_track_ids);
            used_track_ids.insert(id.clone());
            id
        });

        // Duration extracted from source metadata (may differ slightly from encoded output)
        let duration = track_info.duration;

        // Representation-aware directory layout:
        // media/albums/{album_dir}/track_{disc}_{track}/{profile}/manifest.mpd
        let track_dir = output_base.join("albums").join(&album_dir).join(format!(
            "track_{}_{}",
            track_info.disc_number, track_info.track_number
        ));

        let mut representations = Vec::new();
        for profile in profiles {
            let profile_dir = track_dir.join(&profile.name);
            let parsed = parse_track_mpd(&profile_dir)?;

            let (encoded_length_seconds, segment_timescale, segment_timeline, segment_count) =
                if let Some((timescale, timeline, encoded_len, count)) = parsed {
                    (Some(encoded_len), Some(timescale), Some(timeline), count)
                } else {
                    (None, None, None, (duration / 6.0).ceil() as u32)
                };

            let init_segment_path = format!(
                "albums/{}/track_{}_{}/{}/init.mp4",
                album_dir, track_info.disc_number, track_info.track_number, profile.name
            );
            let segment_path_template = format!(
                "albums/{}/track_{}_{}/{}/chunk_%03d$.m4s",
                album_dir, track_info.disc_number, track_info.track_number, profile.name
            );

            representations.push(TrackRepresentation {
                name: profile.name.clone(),
                adaptation_set_id: profile.adaptation_set_id.clone(),
                codec: profile.codec.clone(),
                mpd_codecs: mpd_codecs_for_profile(profile)?,
                bandwidth: bitrate_to_bps(&profile.bitrate)?,
                sample_rate: profile.sample_rate,
                channels: profile.channels,
                init_segment_path,
                segment_path_template,
                segment_count,
                segment_timescale,
                segment_timeline,
                encoded_length_seconds,
            });
        }

        // Validate that representations are aligned *within the same adaptation set*.
        // Different codecs (different AdaptationSets) are allowed to have different segment
        // boundaries/timelines.
        if !representations.is_empty() {
            let mut groups: std::collections::BTreeMap<String, Vec<&TrackRepresentation>> =
                std::collections::BTreeMap::new();
            for rep in &representations {
                let key = rep.adaptation_set_id.clone().unwrap_or_else(|| {
                    format!("legacy:{}:{}:{}", rep.codec, rep.sample_rate, rep.channels)
                });
                groups.entry(key).or_default().push(rep);
            }

            for (set_id, reps_in_set) in groups {
                if reps_in_set.len() < 2 {
                    continue;
                }
                let first = reps_in_set[0];
                for rep in reps_in_set.iter().skip(1) {
                    if rep.segment_count != first.segment_count {
                        return Err(anyhow!(
                            "Representation {} segment_count mismatch in set {} for track {}: {} != {}",
                            rep.name,
                            set_id,
                            track_id,
                            rep.segment_count,
                            first.segment_count
                        ));
                    }

                    // Prefer strict timeline equality when present (best signal for bitrate switching).
                    if let (Some(a_ts), Some(a_tl), Some(b_ts), Some(b_tl)) = (
                        rep.segment_timescale,
                        rep.segment_timeline.as_ref(),
                        first.segment_timescale,
                        first.segment_timeline.as_ref(),
                    ) {
                        if a_ts != b_ts || a_tl != b_tl {
                            return Err(anyhow!(
                                "Representation {} SegmentTimeline mismatch in set {} for track {}",
                                rep.name,
                                set_id,
                                track_id
                            ));
                        }
                    } else if let (Some(a), Some(b)) =
                        (rep.encoded_length_seconds, first.encoded_length_seconds)
                        && (a - b).abs() > 0.05
                    {
                        return Err(anyhow!(
                            "Representation {} duration mismatch in set {} for track {}: {:.3}s != {:.3}s",
                            rep.name,
                            set_id,
                            track_id,
                            a,
                            b
                        ));
                    }
                }
            }
        }

        tracks.push(Track {
            id: track_id,
            title: track_info.title.clone(),
            artist: Some(track_info.artist.clone()),
            disc_number: track_info.disc_number,
            track_number: track_info.track_number,
            length_seconds: duration,
            representations,
            init_segment_path: None,
            segment_path_template: None,
            segment_count: None,
            segment_duration: None,
            source_file_path: Some(track_info.path.display().to_string()),
            created_at: existing_track
                .map(|t| t.created_at.clone())
                .unwrap_or_else(|| now.clone()),
        });
    }

    // Calculate disc count from tracks
    let disc_count = tracks.iter().map(|t| t.disc_number).max().unwrap_or(1);

    // Create album (preserve created_at when updating). "enabled" is runtime-only.
    let album = Album {
        id: album_id,
        name: album_info.name,
        artist: album_info.artist,
        release_date: album_info.release_date,
        original_release_date: album_info.original_release_date,
        media: album_info.media,
        label: album_info.label,
        catalog_number: album_info.catalog_number,
        musicbrainz_album_id: album_info.musicbrainz_album_id,
        disc_count,
        cover_path,
        created_at: existing_album
            .as_ref()
            .map(|a| a.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
        images,
        tracks,
    };

    // Write album YAML (rename old file if album name/artist changed).
    let album_dir = album_dir_name(&album.id, &album.artist, &album.name);
    let target_path = metadata_dir.join(format!("{}.yaml", album_dir));

    if let Some(existing_path) = find_album_yaml_by_id(metadata_dir, &album.id)?
        && existing_path != target_path
    {
        std::fs::rename(&existing_path, &target_path).with_context(|| {
            format!(
                "Rename album metadata {} -> {}",
                existing_path.display(),
                target_path.display()
            )
        })?;
    }

    let yaml = serde_yaml::to_string(&album)?;
    std::fs::write(&target_path, yaml)
        .with_context(|| format!("Write album metadata {}", target_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_track_mpd_fallback_duration() {
        let tmp = tempfile::tempdir().unwrap();
        let mpd = r#"<?xml version="1.0" encoding="utf-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     type="static"
     mediaPresentationDuration="PT2M45.2S">
  <Period id="0" start="PT0S">
    <AdaptationSet>
      <Representation>
        <SegmentTemplate timescale="1000000" duration="6000000" initialization="init.mp4" media="chunk_$Number%03d$.m4s" startNumber="1">
        </SegmentTemplate>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>
"#;
        fs::write(tmp.path().join("manifest.mpd"), mpd).unwrap();

        let parsed = parse_track_mpd(tmp.path()).unwrap().unwrap();
        assert_eq!(parsed.0, 1_000_000);
        assert!((parsed.2 - 165.2).abs() < 0.0001);
        assert_eq!(parsed.3, 28);

        let entries = parsed.1;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].t, Some(0));
        assert_eq!(entries[0].d, 6_000_000);
        assert_eq!(entries[0].r, Some(26));
        assert_eq!(entries[1].t, None);
        assert_eq!(entries[1].d, 3_200_000);
        assert_eq!(entries[1].r, None);
    }

    #[test]
    fn test_parse_track_mpd_segment_timeline() {
        let tmp = tempfile::tempdir().unwrap();
        let mpd = r#"<?xml version="1.0" encoding="utf-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     type="static"
     mediaPresentationDuration="PT13S">
  <Period id="0" start="PT0S">
    <AdaptationSet>
      <Representation>
        <SegmentTemplate timescale="1000" initialization="init.mp4" media="chunk_$Number%03d$.m4s" startNumber="1">
          <SegmentTimeline>
            <S t="0" d="5000" r="1" />
            <S d="3000" />
          </SegmentTimeline>
        </SegmentTemplate>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>
"#;
        fs::write(tmp.path().join("manifest.mpd"), mpd).unwrap();

        let parsed = parse_track_mpd(tmp.path()).unwrap().unwrap();
        assert_eq!(parsed.0, 1000);
        assert_eq!(parsed.3, 3);

        let entries = parsed.1;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].t, Some(0));
        assert_eq!(entries[0].d, 5000);
        assert_eq!(entries[0].r, Some(1));
        assert_eq!(entries[1].d, 3000);
    }
}
