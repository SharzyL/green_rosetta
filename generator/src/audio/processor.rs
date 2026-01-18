use anyhow::{anyhow, Result};
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

    /// Process all tracks in an album
    pub async fn process_album(
        &self,
        album: &AlbumInfo,
        output_base: &Path,
        album_id: &str,
        job_semaphore: Arc<Semaphore>,
    ) -> Result<()> {
        println!(
            "\n[{} | {}] Start encoding the album...",
            album_id, album.name
        );

        // Encode tracks in parallel (bounded by `job_semaphore`), so a single large album
        // can still saturate the configured job count.
        let mut join_set = JoinSet::new();
        let total_tracks = album.tracks.len();
        for (idx, track) in album.tracks.iter().cloned().enumerate() {
            let processor = self.clone();
            let album = album.clone();
            let album_id = album_id.to_string();
            let output_base = output_base.to_path_buf();
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
                        &output_base,
                        &album_id,
                        Some((idx + 1, total_tracks)),
                    )
                    .await
            });
        }

        while let Some(result) = join_set.join_next().await {
            result??;
        }

        Ok(())
    }

    /// Encode a single track to CMAF segments
    async fn encode_track(
        &self,
        album: &AlbumInfo,
        track: &crate::scan::TrackInfo,
        output_base: &Path,
        album_id: &str,
        progress: Option<(usize, usize)>,
    ) -> Result<()> {
        // Create output directory:
        // media/albums/{id} - {artist} - {album_name}/track_{disc}_{track}/{profile_name}/
        let artist_dir = album.artist.replace(" ", "_");
        let album_name = album.name.replace(" ", "_");
        let album_dir = output_base.join(format!(
            "albums/{} - {} - {}",
            album_id, artist_dir, album_name
        ));
        let track_dir = album_dir.join(format!(
            "track_{}_{}",
            track.disc_number, track.track_number
        ));

        // Get track duration first
        let duration = self.get_duration(&track.path).await?;
        let segment_count = (duration / self.segment_duration).ceil() as u32;

        if let Some((idx, total)) = progress {
            println!(
                "[{} | {}] [{}/{}] {} - Duration: {:.1}s",
                album_id,
                album.name,
                idx,
                total,
                track.title,
                duration,
            );
        } else {
            println!(
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

            // FFmpeg command for CMAF encoding.
            let cmd = format!(
                r#"ffmpeg -i "{}" \
  -vn \
  -c:a {} \
  -b:a {} \
  -ar {} \
  -ac {} \
  -f dash \
  -use_timeline 1 \
  -seg_duration {} \
  -frag_duration {} \
  -use_template 1 \
  -dash_segment_type mp4 \
  -init_seg_name 'init.mp4' \
  -media_seg_name 'chunk_$Number%03d$.m4s' \
  -ldash 1 \
  -write_prft 1 \
  -movflags +frag_keyframe+empty_moov+default_base_moof+dash \
  -strict experimental \
  "{}/manifest.mpd" \
  -loglevel error -y"#,
                track.path.display(),
                ffmpeg_codec,
                profile.bitrate,
                profile.sample_rate,
                profile.channels,
                self.segment_duration,
                self.segment_duration,
                profile_dir.display()
            );

            let status = Command::new("sh").arg("-c").arg(&cmd).output().await?;

            if !status.status.success() {
                let stderr = String::from_utf8_lossy(&status.stderr);
                return Err(anyhow!(
                    "FFmpeg encoding failed (profile {}): {}",
                    profile.name,
                    stderr.lines().last().unwrap_or("Unknown error")
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

        println!(
            "✓ [{} | {}] {} Generated {} segments ({} profiles)",
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
        let cmd = format!(
            r#"ffprobe -v quiet -print_format json -show_format "{}" "#,
            path.display()
        );

        let output = Command::new("sh").arg("-c").arg(&cmd).output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("ffprobe failed: {}", stderr));
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
