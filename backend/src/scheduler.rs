use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::db::{Album, Database, Track};

#[derive(Debug, Clone)]
struct ScheduledTrack {
    album_id: String,
    track_id: String,
    start_seconds: f64,    // seconds since mpd_start_time
    duration_seconds: f64, // encoded duration
}

impl ScheduledTrack {
    fn end_seconds(&self) -> f64 {
        self.start_seconds + self.duration_seconds
    }
}

pub struct Scheduler {
    mpd_start_time: DateTime<Utc>,
    queue: Arc<tokio::sync::RwLock<VecDeque<ScheduledTrack>>>,
    db: Arc<tokio::sync::RwLock<Database>>,
}

impl Scheduler {
    pub async fn new(
        db: Arc<tokio::sync::RwLock<Database>>,
        suggested_presentation_delay_seconds: f64,
    ) -> anyhow::Result<Self> {
        let (
            first_album_id,
            first_album_name,
            _first_album_artist,
            first_track_id,
            first_track_artist,
            first_track_title,
            first_track_duration,
        ) = {
            let db_read = db.read().await;
            let enabled_albums = db_read.get_enabled_albums();
            if enabled_albums.is_empty() {
                return Err(anyhow::anyhow!("No enabled albums in database"));
            }

            // Randomly select starting album, then start at its first track.
            let mut rng = rand::thread_rng();
            let first_album = enabled_albums
                .choose(&mut rng)
                .ok_or_else(|| anyhow::anyhow!("Failed to select random album"))?;
            let first_track = first_album
                .tracks
                .first()
                .ok_or_else(|| anyhow::anyhow!("Album has no tracks"))?;

            (
                first_album.id.clone(),
                first_album.name.clone(),
                first_album.artist.clone(),
                first_track.id.clone(),
                first_track
                    .artist
                    .clone()
                    .unwrap_or_else(|| first_album.artist.clone()),
                first_track.title.clone(),
                first_track.encoded_duration_seconds(),
            )
        };

        tracing::info!(
            "🎵 Starting playback (random): {} - {} (Album: {})",
            first_track_artist,
            first_track_title,
            first_album_name
        );

        // Shift availabilityStartTime backwards by SPD so new clients starting at (live_edge - SPD)
        // have immediately available media on server startup.
        let offset_ms = (suggested_presentation_delay_seconds.max(0.0) * 1000.0) as i64;
        let mpd_start_time = Utc::now() - chrono::Duration::milliseconds(offset_ms);
        let mut queue = VecDeque::new();
        queue.push_back(ScheduledTrack {
            album_id: first_album_id,
            track_id: first_track_id,
            start_seconds: 0.0,
            duration_seconds: first_track_duration,
        });

        Ok(Scheduler {
            mpd_start_time,
            queue: Arc::new(tokio::sync::RwLock::new(queue)),
            db,
        })
    }

    pub fn mpd_start_time(&self) -> DateTime<Utc> {
        self.mpd_start_time
    }

    pub fn t_now_seconds(&self) -> f64 {
        (Utc::now() - self.mpd_start_time).num_milliseconds() as f64 / 1000.0
    }

    async fn get_album_track(
        &self,
        album_id: &str,
        track_id: &str,
    ) -> anyhow::Result<(Album, Track)> {
        let db = self.db.read().await;
        let album = db
            .get_album(album_id)
            .ok_or_else(|| anyhow::anyhow!("Album not found: {}", album_id))?
            .clone();
        let track = album
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .ok_or_else(|| anyhow::anyhow!("Track not found: {}", track_id))?
            .clone();
        Ok((album, track))
    }

    async fn next_track_in_playlist(
        &self,
        current_album_id: &str,
        current_track_id: &str,
    ) -> anyhow::Result<(String, String)> {
        let db = self.db.read().await;
        let current_album = db
            .get_album(current_album_id)
            .ok_or_else(|| anyhow::anyhow!("Album not found: {}", current_album_id))?;

        // Only continue within the current album if it is still enabled.
        if current_album.enabled
            && let Some(pos) = current_album
                .tracks
                .iter()
                .position(|t| t.id == current_track_id)
            && pos + 1 < current_album.tracks.len()
        {
            let next = &current_album.tracks[pos + 1];
            return Ok((current_album_id.to_string(), next.id.clone()));
        }

        // Move to the next enabled album in list order, wrapping around.
        let enabled = db.get_enabled_albums();
        if enabled.is_empty() {
            return Err(anyhow::anyhow!("No enabled albums"));
        }

        // If the current album isn't enabled, just pick the first enabled album.
        if !current_album.enabled {
            let first_album = enabled[0];
            let first_track = first_album
                .tracks
                .first()
                .ok_or_else(|| anyhow::anyhow!("Enabled album has no tracks"))?;
            return Ok((first_album.id.clone(), first_track.id.clone()));
        }

        let mut found = false;
        for album in &enabled {
            if found && let Some(first_track) = album.tracks.first() {
                return Ok((album.id.clone(), first_track.id.clone()));
            }
            if album.id == current_album_id {
                found = true;
            }
        }

        // Wrap to the first enabled album.
        let first_album = enabled[0];
        let first_track = first_album
            .tracks
            .first()
            .ok_or_else(|| anyhow::anyhow!("Enabled album has no tracks"))?;
        Ok((first_album.id.clone(), first_track.id.clone()))
    }

    async fn append_next(&self) -> anyhow::Result<()> {
        let last = {
            let q = self.queue.read().await;
            q.back()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Schedule queue is empty"))?
        };

        let (next_album_id, next_track_id) = self
            .next_track_in_playlist(&last.album_id, &last.track_id)
            .await?;
        let (_album, track) = self.get_album_track(&next_album_id, &next_track_id).await?;

        let duration = track.encoded_duration_seconds();
        let next = ScheduledTrack {
            album_id: next_album_id,
            track_id: next_track_id,
            start_seconds: last.end_seconds(),
            duration_seconds: duration,
        };

        self.queue.write().await.push_back(next);
        Ok(())
    }

    /// Ensure the schedule covers a window of interest.
    ///
    /// - Keep past coverage back to `t_now - time_shift_buffer_depth`.
    /// - Ensure future coverage out to `t_now + min_future_manifest_duration`.
    /// - Pop old tracks only when fully outside the past window.
    pub async fn maintain(
        &self,
        time_shift_buffer_depth: f64,
        min_future_manifest_duration: f64,
    ) -> anyhow::Result<()> {
        let t_now = self.t_now_seconds();
        let window_start = (t_now - time_shift_buffer_depth).max(0.0);
        let window_end = t_now + min_future_manifest_duration.max(0.0);

        // Ensure the schedule reaches far enough into the future.
        loop {
            let needs_more = {
                let q = self.queue.read().await;
                q.back()
                    .map(|t| t.end_seconds() < window_end)
                    .unwrap_or(true)
            };
            if !needs_more {
                break;
            }
            self.append_next().await?;
        }

        // Ensure we still cover window_start (in case the server has been up a long time).
        loop {
            let covers_start = {
                let q = self.queue.read().await;
                q.back()
                    .map(|t| t.end_seconds() >= window_start)
                    .unwrap_or(false)
            };
            if covers_start {
                break;
            }
            self.append_next().await?;
        }

        // Drop tracks that are fully older than the window.
        loop {
            let should_pop = {
                let q = self.queue.read().await;
                q.front()
                    .map(|t| t.end_seconds() < window_start)
                    .unwrap_or(false)
            };
            if !should_pop {
                break;
            }
            self.queue.write().await.pop_front();
        }

        Ok(())
    }

    pub async fn get_mpd_periods(
        &self,
        time_shift_buffer_depth: f64,
        min_future_manifest_duration: f64,
    ) -> anyhow::Result<Vec<(Track, f64, f64)>> {
        self.maintain(time_shift_buffer_depth, min_future_manifest_duration)
            .await?;

        let t_now = self.t_now_seconds();
        let window_start = (t_now - time_shift_buffer_depth).max(0.0);
        let window_end = t_now + min_future_manifest_duration.max(0.0);

        // Avoid holding the queue lock across `.await`.
        let scheduled: Vec<ScheduledTrack> = {
            let q = self.queue.read().await;
            q.iter()
                .filter(|st| st.end_seconds() > window_start && st.start_seconds < window_end)
                .cloned()
                .collect()
        };

        let mut out = Vec::with_capacity(scheduled.len());
        for st in scheduled {
            let (_album, track) = self.get_album_track(&st.album_id, &st.track_id).await?;
            out.push((track, st.start_seconds, st.duration_seconds));
        }
        Ok(out)
    }
}
