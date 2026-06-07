use anyhow::{Result, anyhow};
use std::path::Path;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::scan::AlbumInfo;
use crate::{config, config::AudioProfile};

#[derive(Clone)]
pub struct AudioProcessor {
    segment_duration: f64,
    profiles: Vec<AudioProfile>,
}

impl AudioProcessor {
    pub fn new(segment_duration: f64, profiles: Vec<AudioProfile>) -> Self {
        AudioProcessor {
            segment_duration,
            profiles,
        }
    }

    /// Process all tracks in an album, writing into `album_dir` (caller-controlled).
    ///
    /// The caller is expected to use a staging path here and rename to the final location
    /// on success, so a failed album leaves only the staging dir behind.
    pub async fn process_album(
        &self,
        album: &AlbumInfo,
        album_dir: &Path,
        album_id: &str,
        job_semaphore: Arc<Semaphore>,
    ) -> Result<()> {
        tracing::info!(
            "[{} | {}] Start encoding the album...",
            album_id,
            album.name
        );

        // Encode tracks in parallel (bounded by `job_semaphore`), so a single large album
        // can still saturate the configured job count.
        let mut join_set = JoinSet::new();
        let total_tracks = album.tracks.len();
        for (idx, track) in album.tracks.iter().cloned().enumerate() {
            let processor = self.clone();
            let album = album.clone();
            let album_id = album_id.to_string();
            let album_dir = album_dir.to_path_buf();
            let job_semaphore = job_semaphore.clone();

            join_set.spawn(async move {
                let _permit = job_semaphore
                    .acquire()
                    .await
                    .expect("semaphore should not be closed");

                processor
                    .encode_track(
                        &album,
                        &track,
                        &album_dir,
                        &album_id,
                        Some((idx + 1, total_tracks)),
                    )
                    .await
            });
        }

        // On the first error, drop the JoinSet which aborts the remaining tasks; combined
        // with `kill_on_drop(true)` on the ffmpeg/ffprobe Commands, this terminates any
        // in-flight subprocesses so they can't keep writing into the staging dir.
        while let Some(result) = join_set.join_next().await {
            result??;
        }

        Ok(())
    }

    /// Encode a single track to CMAF segments under `album_dir/track_{disc}_{track}/`.
    async fn encode_track(
        &self,
        album: &AlbumInfo,
        track: &crate::scan::TrackInfo,
        album_dir: &Path,
        album_id: &str,
        progress: Option<(usize, usize)>,
    ) -> Result<()> {
        let track_dir = album_dir.join(format!(
            "track_{}_{}",
            track.disc_number, track.track_number
        ));

        // Get track duration first
        let duration = self.get_duration(&track.path).await?;
        let segment_count = (duration / self.segment_duration).ceil() as u32;

        if let Some((idx, total)) = progress {
            tracing::info!(
                "[{} | {}] [{}/{}] {} - Duration: {:.1}s",
                album_id,
                album.name,
                idx,
                total,
                track.title,
                duration,
            );
        } else {
            tracing::info!(
                "[{} | {}] {} - Duration: {:.1}s",
                album_id,
                album.name,
                track.title,
                duration,
            );
        }

        for profile in &self.profiles {
            let profile_dir = track_dir.join(&profile.name);
            std::fs::create_dir_all(&profile_dir)?;

            let ffmpeg_codec = config::ffmpeg_codec_for_profile(profile)?;

            let input_path = track
                .path
                .to_str()
                .ok_or_else(|| anyhow!("Non-UTF-8 input path: {}", track.path.display()))?;
            let manifest_path = profile_dir.join("manifest.mpd");
            let manifest_arg = manifest_path
                .to_str()
                .ok_or_else(|| anyhow!("Non-UTF-8 manifest path: {}", manifest_path.display()))?;
            let seg_duration = self.segment_duration.to_string();

            let output = Command::new("ffmpeg")
                .kill_on_drop(true)
                .args([
                    "-i",
                    input_path,
                    "-vn",
                    "-c:a",
                    ffmpeg_codec,
                    "-b:a",
                    &profile.bitrate,
                    "-ar",
                    &profile.sample_rate.to_string(),
                    "-ac",
                    &profile.channels.to_string(),
                    "-f",
                    "dash",
                    "-use_timeline",
                    "1",
                    "-seg_duration",
                    &seg_duration,
                    "-frag_duration",
                    &seg_duration,
                    "-use_template",
                    "1",
                    "-dash_segment_type",
                    "mp4",
                    "-init_seg_name",
                    "init.mp4",
                    "-media_seg_name",
                    "chunk_$Number%03d$.m4s",
                    "-ldash",
                    "1",
                    "-write_prft",
                    "1",
                    "-movflags",
                    "+frag_keyframe+empty_moov+default_base_moof+dash",
                    "-strict",
                    "experimental",
                    manifest_arg,
                    "-loglevel",
                    "error",
                    "-y",
                ])
                .output()
                .await
                .map_err(|e| {
                    anyhow!(
                        "Failed to spawn ffmpeg for {} (profile {}): {}",
                        track.path.display(),
                        profile.name,
                        e
                    )
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow!(
                    "ffmpeg failed on {} (profile {}, exit {}): {}",
                    track.path.display(),
                    profile.name,
                    output.status,
                    stderr.lines().last().unwrap_or("no stderr output").trim(),
                ));
            }

            // Verify generated files
            let init_path = profile_dir.join("init.mp4");
            if !init_path.exists() {
                return Err(anyhow!(
                    "FFmpeg did not generate init.mp4 (profile {})",
                    profile.name
                ));
            }
        }

        tracing::info!(
            "[{} | {}] {} Generated {} segments ({} profiles)",
            album_id,
            album.name,
            track.title,
            segment_count,
            self.profiles.len(),
        );

        Ok(())
    }

    /// Get audio file duration using ffprobe
    async fn get_duration(&self, path: &Path) -> Result<f64> {
        let output = Command::new("ffprobe")
            .kill_on_drop(true)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                path.to_str()
                    .ok_or_else(|| anyhow!("Non-UTF-8 file path: {}", path.display()))?,
            ])
            .output()
            .await
            .map_err(|e| anyhow!("Failed to spawn ffprobe on {}: {}", path.display(), e))?;

        if !output.status.success() {
            return Err(anyhow!(
                "ffprobe failed on {} (exit {}): {}",
                path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        // Parse JSON to get duration
        let json: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| anyhow!("Failed to parse ffprobe output: {}", e))?;

        let duration = json["format"]["duration"]
            .as_str()
            .and_then(|d| d.parse::<f64>().ok())
            .ok_or_else(|| anyhow!("Could not find duration in ffprobe output"))?;

        if duration <= 0.0 {
            return Err(anyhow!("Invalid duration: {}", duration));
        }

        Ok(duration)
    }
}
