use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use std::collections::{HashSet, VecDeque};
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
    /// The last track ever appended, retained after it leaves `queue`.
    ///
    /// `queue` is trimmed to the manifest window, so once a quiet spell ages every entry out it
    /// can no longer answer "where has the catalogue reached". This cursor can.
    last_scheduled: Arc<tokio::sync::RwLock<Option<ScheduledTrack>>>,
    /// Serialises `maintain` passes against each other.
    maintain_lock: Arc<tokio::sync::Mutex<()>>,
    db: Arc<tokio::sync::RwLock<Database>>,
}

/// A point-in-time view of which albums the scheduler currently has on air or queued ahead.
#[derive(Debug, Clone, Default)]
pub struct SchedulingSnapshot {
    /// Album whose track spans the current playback instant, if any.
    pub now_playing_album_id: Option<String>,
    /// Albums with tracks scheduled in the future part of the window (excludes the now-playing
    /// track; an album may still be both now-playing and queued, which `now_playing` takes
    /// precedence over when rendering a single badge).
    pub queued_album_ids: HashSet<String>,
}

/// Pick a random enabled album and its first track.
///
/// Takes an already-locked database so callers holding the guard do not re-acquire it, and hands
/// back references so they can read whatever else they need (name, title, duration) off them.
fn pick_random_start(db: &Database) -> anyhow::Result<(&Album, &Track)> {
    let enabled = db.get_enabled_albums();
    let album = {
        let mut rng = rand::thread_rng();
        *enabled
            .choose(&mut rng)
            .ok_or_else(|| anyhow::anyhow!("No enabled albums"))?
    };
    let track = album
        .tracks
        .first()
        .ok_or_else(|| anyhow::anyhow!("Enabled album has no tracks"))?;
    Ok((album, track))
}

impl Scheduler {
    pub async fn new(
        db: Arc<tokio::sync::RwLock<Database>>,
        suggested_presentation_delay_seconds: f64,
    ) -> anyhow::Result<Self> {
        let (
            first_album_id,
            first_album_name,
            first_track_id,
            first_track_artist,
            first_track_title,
            first_track_duration,
        ) = {
            let db_read = db.read().await;
            let (album, track) = pick_random_start(&db_read)?;

            (
                album.id.clone(),
                album.name.clone(),
                track.id.clone(),
                track.artist.clone().unwrap_or_else(|| album.artist.clone()),
                track.title.clone(),
                track.encoded_duration_seconds(),
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
        let first = ScheduledTrack {
            album_id: first_album_id,
            track_id: first_track_id,
            start_seconds: 0.0,
            duration_seconds: first_track_duration,
        };
        let mut queue = VecDeque::new();
        queue.push_back(first.clone());

        Ok(Scheduler {
            mpd_start_time,
            queue: Arc::new(tokio::sync::RwLock::new(queue)),
            last_scheduled: Arc::new(tokio::sync::RwLock::new(Some(first))),
            maintain_lock: Arc::new(tokio::sync::Mutex::new(())),
            db,
        })
    }

    pub fn mpd_start_time(&self) -> DateTime<Utc> {
        self.mpd_start_time
    }

    /// Test-only: move `mpd_start_time` back so `t_now_seconds()` jumps forward, simulating a
    /// stretch of wall-clock time passing without sleeping through it.
    #[cfg(test)]
    fn advance_clock(&mut self, seconds: i64) {
        self.mpd_start_time -= chrono::Duration::seconds(seconds);
    }

    /// Test-only: the playlist cursor, as (album_id, track_id).
    #[cfg(test)]
    async fn last_scheduled_ids(&self) -> Option<(String, String)> {
        self.last_scheduled
            .read()
            .await
            .as_ref()
            .map(|st| (st.album_id.clone(), st.track_id.clone()))
    }

    pub fn t_now_seconds(&self) -> f64 {
        (Utc::now() - self.mpd_start_time).num_milliseconds() as f64 / 1000.0
    }

    /// Snapshot which albums are currently playing / queued ahead, based on the current queue.
    ///
    /// Relies on `maintain` having been run for the current window (the request middleware does
    /// this on every request), so the future part of the queue is already populated. This is a
    /// momentary view: the now-playing album advances over time, so callers should re-query.
    pub async fn scheduling_snapshot(&self) -> SchedulingSnapshot {
        let t_now = self.t_now_seconds();
        let q = self.queue.read().await;
        let mut snap = SchedulingSnapshot::default();
        for st in q.iter() {
            if st.start_seconds <= t_now && t_now < st.end_seconds() {
                snap.now_playing_album_id = Some(st.album_id.clone());
            } else if st.start_seconds > t_now {
                snap.queued_album_ids.insert(st.album_id.clone());
            }
        }
        snap
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
        // The current album may have been removed by a metadata reload. Treat a missing album the
        // same as a disabled one: fall through to picking the next enabled album.
        let current_album = db.get_album(current_album_id);

        // Only continue within the current album if it still exists and is enabled.
        if let Some(current_album) = current_album
            && current_album.enabled
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

        // If the current album is gone or disabled, just pick the first enabled album.
        if current_album.is_none_or(|a| !a.enabled) {
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

        self.push_scheduled(next).await;
        Ok(())
    }

    /// Append to the window and advance the playlist cursor together, so the cursor outlives the
    /// window entry it came from.
    async fn push_scheduled(&self, scheduled: ScheduledTrack) {
        *self.last_scheduled.write().await = Some(scheduled.clone());
        self.queue.write().await.push_back(scheduled);
    }

    /// Drop queued tracks whose album/track no longer exists (e.g. a reload deleted the album).
    ///
    /// Without this, a removed in-window track keeps "covering" its time slot, so `maintain`
    /// never backfills it and `get_mpd_periods` renders nothing for that span. Pruning frees the
    /// window so the future-coverage loop refills it with still-existing albums.
    async fn prune_unresolvable(&self) {
        // Common case touches only read locks; escalate to a write lock only when a track
        // actually needs removing.
        let needs_prune = {
            let q = self.queue.read().await;
            let db = self.db.read().await;
            q.iter().any(|st| !db.has_track(&st.album_id, &st.track_id))
        };
        if !needs_prune {
            return;
        }

        let mut q = self.queue.write().await;
        let db = self.db.read().await;
        let before = q.len();
        q.retain(|st| db.has_track(&st.album_id, &st.track_id));
        let removed = before - q.len();
        if removed > 0 {
            tracing::warn!(
                "Pruned {removed} unresolvable scheduled track(s) after metadata change"
            );
        }
    }

    /// Pick a random enabled album's first track, the way `new` chooses a cold start.
    async fn random_start(&self) -> anyhow::Result<(String, String)> {
        let db = self.db.read().await;
        let (album, track) = pick_random_start(&db)?;
        Ok((album.id.clone(), track.id.clone()))
    }

    /// Re-enter the playlist at `start_seconds` after the queue ran dry.
    ///
    /// Resumes from `last_scheduled` so an interruption skips the dead span instead of rewinding
    /// to the top of the album list. Only a genuine cold start has no cursor to follow.
    ///
    /// Reaching here is abnormal: the periodic tick maintains the window regardless of traffic,
    /// so a drained queue means maintenance stopped for longer than the window (a stalled
    /// runtime, a suspended host, repeated `maintain` failures) or pruning removed every entry.
    /// We recover rather than fail the manifest, but say so loudly.
    async fn resume_at(&self, start_seconds: f64) -> anyhow::Result<()> {
        let cursor = self.last_scheduled.read().await.clone();
        let (album_id, track_id) = match cursor {
            Some(last) => {
                // Negative means the queue was emptied by pruning while the cursor still pointed
                // ahead of now; positive is dead air nobody could have been served.
                let gap_seconds = start_seconds - last.end_seconds();
                tracing::warn!(
                    gap_seconds,
                    after_album_id = %last.album_id,
                    after_track_id = %last.track_id,
                    "schedule queue ran dry; rejoining the playlist at the live edge"
                );
                self.next_track_in_playlist(&last.album_id, &last.track_id)
                    .await?
            }
            None => {
                tracing::warn!(
                    "schedule queue ran dry with no playlist cursor; starting from a random album"
                );
                self.random_start().await?
            }
        };

        let (_album, track) = self.get_album_track(&album_id, &track_id).await?;
        self.push_scheduled(ScheduledTrack {
            album_id,
            track_id,
            start_seconds,
            duration_seconds: track.encoded_duration_seconds(),
        })
        .await;
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
        // `append_next` reads the queue tail, awaits the database, then pushes. Two passes
        // running that concurrently -- a request and the periodic tick, on a multi-threaded
        // runtime -- would extend from the same tail and emit duplicate periods at one start
        // time. Serialising the whole pass also keeps `last_scheduled` in step with the tail,
        // since every push happens under this guard.
        let _guard = self.maintain_lock.lock().await;

        let t_now = self.t_now_seconds();
        let window_start = (t_now - time_shift_buffer_depth).max(0.0);
        let window_end = t_now + min_future_manifest_duration.max(0.0);

        // Remove tracks that no longer exist (e.g. a reload deleted their album) so they don't
        // block backfill below or render as empty periods.
        self.prune_unresolvable().await;

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

        // If pruning/dropping emptied the queue (e.g. the only queued album was deleted, or the
        // process was stalled long enough for the whole window to age out), rejoin the playlist
        // at the live edge so the stream recovers instead of going silent.
        if self.queue.read().await.is_empty() {
            self.resume_at(t_now).await?;
        }

        // Ensure the schedule reaches far enough into the future. Appending from the (now valid)
        // back also fills any span freed by pruning, since each track starts where the last ends.
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

        Ok(())
    }

    pub async fn get_mpd_periods(
        &self,
        time_shift_buffer_depth: f64,
        min_future_manifest_duration: f64,
    ) -> anyhow::Result<Vec<(Track, f64, f64)>> {
        // Best-effort: if maintenance fails (e.g. a reload left no enabled albums to schedule),
        // still serve whatever is already in the window rather than failing the whole manifest.
        if let Err(e) = self
            .maintain(time_shift_buffer_depth, min_future_manifest_duration)
            .await
        {
            tracing::warn!("scheduler maintain failed (serving existing window): {e}");
        }

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
            // A track may have vanished from the database after a reload removed its album. Skip
            // that period (leaving a harmless gap at its original start time) instead of failing
            // the whole manifest for every listener. We keep the surviving periods' original
            // start_seconds so the timeline stays anchored and clients don't resync.
            match self.get_album_track(&st.album_id, &st.track_id).await {
                Ok((_album, track)) => out.push((track, st.start_seconds, st.duration_seconds)),
                Err(e) => {
                    tracing::warn!(
                        album_id = %st.album_id,
                        track_id = %st.track_id,
                        "scheduled track no longer in metadata after reload; skipping period: {e}"
                    );
                    continue;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Album, Track};

    fn test_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            title: id.to_string(),
            artist: None,
            disc_number: 1,
            track_number: 1,
            length_seconds: 12.0,
            representations: vec![],
            // Legacy fields drive encoded_duration_seconds() -> 2 * 6.0 = 12s.
            encoded_length_seconds: None,
            init_segment_path: Some(format!("media/{id}/init.mp4")),
            segment_path_template: Some(format!("media/{id}/chunk_$Number$.m4s")),
            segment_count: Some(2),
            segment_duration: Some(6.0),
            segment_timescale: None,
            segment_timeline: None,
            source_file_path: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn test_album(id: &str, n_tracks: usize) -> Album {
        Album {
            id: id.to_string(),
            name: id.to_string(),
            artist: "Artist".to_string(),
            release_date: None,
            original_release_date: None,
            media: None,
            label: None,
            catalog_number: None,
            musicbrainz_album_id: None,
            disc_count: 1,
            cover_path: "cover.jpg".to_string(),
            enabled: true,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            images: vec![],
            tracks: (0..n_tracks)
                .map(|i| test_track(&format!("{id}_t{i}")))
                .collect(),
        }
    }

    /// Regression: a quiet spell with no requests lets the whole window age out of the queue.
    /// Rejoining must continue the playlist from where it left off, not restart at the first
    /// album -- that made a low-traffic station replay album zero on every visit.
    ///
    /// One album, deliberately: `Scheduler::new` picks its start at random, and across several
    /// short albums the window can reach the end of the list, where continuing legitimately
    /// wraps to the first track -- indistinguishable from the restart this test exists to catch.
    /// A single album longer than the window keeps that ambiguity out.
    #[tokio::test]
    async fn idle_gap_resumes_playlist_instead_of_restarting() {
        let db = Arc::new(tokio::sync::RwLock::new(Database {
            albums: vec![test_album("A", 10)],
        }));
        let mut scheduler = Scheduler::new(db.clone(), 0.0).await.unwrap();

        let (tsbd, min_future) = (60.0, 60.0);
        scheduler.maintain(tsbd, min_future).await.unwrap();

        // Where the catalogue had reached, and which track should follow it.
        let (last_album, last_track) = scheduler.last_scheduled_ids().await.unwrap();
        let (_, expected_track) = scheduler
            .next_track_in_playlist(&last_album, &last_track)
            .await
            .unwrap();

        // Nobody visits for an hour: every queued track now ends far behind the window.
        scheduler.advance_clock(3600);

        let periods = scheduler.get_mpd_periods(tsbd, min_future).await.unwrap();
        assert!(!periods.is_empty(), "manifest went empty after an idle gap");
        assert_eq!(
            periods[0].0.id, expected_track,
            "expected the playlist to continue after the gap"
        );
        assert_ne!(
            periods[0].0.id, "A_t0",
            "restarted at the first album's first track"
        );
    }

    /// Regression: deleting the album that's currently on air (so the whole manifest window is
    /// that album) must not leave an empty manifest — the scheduler should prune it and refill
    /// the window from the remaining albums.
    #[tokio::test]
    async fn manifest_recovers_when_currently_playing_album_is_deleted() {
        let db = Arc::new(tokio::sync::RwLock::new(Database {
            albums: vec![test_album("A", 3), test_album("B", 3), test_album("C", 3)],
        }));
        let scheduler = Scheduler::new(db.clone(), 0.0).await.unwrap();

        let (tsbd, min_future) = (60.0, 30.0);

        let periods = scheduler.get_mpd_periods(tsbd, min_future).await.unwrap();
        assert!(
            !periods.is_empty(),
            "expected a populated manifest to start"
        );

        // Delete whichever album is currently playing.
        let playing = scheduler
            .scheduling_snapshot()
            .await
            .now_playing_album_id
            .expect("an album should be on air");
        {
            let mut w = db.write().await;
            w.albums.retain(|a| a.id != playing);
        }

        let periods = scheduler.get_mpd_periods(tsbd, min_future).await.unwrap();
        assert!(
            !periods.is_empty(),
            "manifest went empty after deleting the currently-playing album"
        );

        // And the deleted album is no longer the one on air.
        let snap = scheduler.scheduling_snapshot().await;
        assert_ne!(snap.now_playing_album_id.as_deref(), Some(playing.as_str()));
    }
}
