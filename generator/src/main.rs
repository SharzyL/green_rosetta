use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};

mod audio;
mod config;
mod db;
mod scan;

use audio::processor::AudioProcessor;
use scan::Scanner;

fn default_jobs() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    // Keep 10% headroom for OS/other work; never drop below 1.
    ((cores as f64) * 0.9).floor().max(1.0) as usize
}

fn album_log_prefix(album_id: &str, album: &scan::AlbumInfo) -> String {
    format!("[{} | {}]", album_id, album.name)
}

fn warn_album_tag_issues(album_id: &str, album: &scan::AlbumInfo) {
    for t in &album.tracks {
        let t_album = t.album.trim();
        if !t_album.is_empty() && t_album != album.name {
            tracing::warn!(
                "album={} name='{}': Different ALBUM tags in one album dir (expected '{}', got '{}') (track: {})",
                album_id,
                album.name,
                album.name,
                t_album,
                t.path.display()
            );
        }

        if t.catalog_number.is_some() && t.media.is_none() {
            tracing::warn!(
                "album={} name='{}': Track has catalog number but no media tag ({}): {}",
                album_id,
                album.name,
                t.path.display(),
                t.title
            );
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "Green Rosetta Generator")]
#[command(about = "Generate CMAF audio chunks and metadata for music radio", long_about = None)]
struct Args {
    /// Enable debug logging (overrides RUST_LOG)
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Generate CMAF segments and per-album metadata for one or more album directories
    Generate {
        /// Input directories.
        ///
        /// - Without `--scan`: each directory is treated as one album directory.
        /// - With `--scan`: each directory is treated as a directory containing album subdirectories.
        #[arg(value_name = "DIR")]
        album_dirs: Vec<PathBuf>,

        /// Treat each positional argument as a directory to scan.
        ///
        /// For each provided directory, we scan its direct subdirectories and treat each as an album.
        #[arg(long)]
        scan: bool,

        /// Root output directory. Generated layout:
        /// - `{output_dir}/media/...` for CMAF segments + cover art
        /// - `{output_dir}/metadata/*.yaml` for per-album metadata
        #[arg(short = 'o', long, default_value = ".")]
        output_dir: PathBuf,

        /// Filter albums by name (regex pattern)
        #[arg(short, long, requires = "scan")]
        filter: Option<String>,

        /// Number of parallel jobs
        #[arg(short = 'j', long)]
        jobs: Option<usize>,

        /// Configuration file path (for audio profiles / segment duration)
        #[arg(long, default_value = "./generator.config.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing after parsing args so CLI flags can control the log level.
    let filter = if args.debug {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    match args.command {
        Commands::Generate {
            album_dirs,
            scan,
            output_dir,
            filter,
            jobs,
            config,
        } => {
            let jobs = jobs.unwrap_or_else(default_jobs);
            generate(album_dirs, scan, output_dir, filter, jobs, config).await?;
        }
    }

    Ok(())
}

async fn generate(
    album_dirs: Vec<PathBuf>,
    scan: bool,
    output_dir: PathBuf,
    filter: Option<String>,
    jobs: usize,
    config_path: PathBuf,
) -> Result<()> {
    let output_media = output_dir.join("media");
    let metadata_dir = output_dir.join("metadata");

    if scan {
        tracing::info!("Generating albums by scanning directories:");
    } else {
        tracing::info!("Generating albums from album directories:");
    }
    for i in &album_dirs {
        tracing::info!("  - {}", i.display());
    }
    tracing::info!("Output dir: {}", output_dir.display());
    tracing::info!("Media dir: {}", output_media.display());
    tracing::info!("Metadata dir: {}", metadata_dir.display());
    tracing::info!("Parallel jobs: {}", jobs);

    if let Some(ref f) = filter {
        tracing::info!("Filter: {}", f);
    }

    let cfg = config::GeneratorConfig::load(&config_path)?;
    let profiles = cfg.encoding_profiles()?;
    config::validate_profiles(&profiles)?;
    let segment_duration_seconds = cfg.segment_duration_seconds();

    // Ensure output directories exist.
    std::fs::create_dir_all(&output_media)?;
    std::fs::create_dir_all(&metadata_dir)?;

    if album_dirs.is_empty() {
        return Err(anyhow::anyhow!(
            "No input directories provided. Pass one or more directories."
        ));
    }

    // Scan all albums.
    let scanner = Scanner::new();
    let mut albums = Vec::new();
    if scan {
        for scan_root in &album_dirs {
            match scanner.scan_directory(scan_root) {
                Ok(mut scanned) => albums.append(&mut scanned),
                Err(e) => {
                    tracing::warn!(
                        "Failed to scan directory {} with --scan: {}",
                        scan_root.display(),
                        e
                    );
                }
            }
        }
    } else {
        for album_dir in &album_dirs {
            match scanner.scan_album(album_dir) {
                Ok(album) => albums.push(album),
                Err(e) => {
                    tracing::warn!("Failed to scan album dir {}: {}", album_dir.display(), e);
                }
            }
        }
    }

    // Apply filter if provided
    if let Some(filter_pattern) = filter {
        let re = regex::Regex::new(&filter_pattern)?;
        let original_count = albums.len();
        albums.retain(|album| {
            re.is_match(&album.artist)
                || re.is_match(&album.name)
                || re.is_match(&format!("{} - {}", album.artist, album.name))
        });
        tracing::info!(
            "Filtered to {} albums (from {})",
            albums.len(),
            original_count
        );
    } else {
        tracing::info!("Found {} albums", albums.len());
    }

    if albums.is_empty() {
        return Err(anyhow::anyhow!("No albums match the filter"));
    }

    // Pre-generate album IDs for all albums (serially), so track processing can run in parallel.
    let mut album_ids = Vec::with_capacity(albums.len());
    for album in &albums {
        let id = db::importer::determine_album_id(&metadata_dir, album, None)?;
        album_ids.push(id);
    }

    // Serialize metadata writes to avoid concurrent file updates/id collisions.
    let metadata_lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));

    // Global job limiter: bounds concurrent track encodes (ffmpeg processes).
    let job_semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(jobs));
    let total = albums.len();
    // Track completion count so the progress indicator reflects completed work, not started work.
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Process albums in parallel
    let mut tasks = Vec::new();

    for (album_info, album_id) in albums.into_iter().zip(album_ids) {
        let output_media = output_media.clone();
        let metadata_dir = metadata_dir.clone();
        let completed = completed.clone();
        let profiles = profiles.clone();
        let metadata_lock = metadata_lock.clone();
        let job_semaphore = job_semaphore.clone();

        let task = tokio::spawn(async move {
            tracing::info!(
                "{} Processing: {} by {}",
                album_log_prefix(&album_id, &album_info),
                album_info.name,
                album_info.artist
            );
            warn_album_tag_issues(&album_id, &album_info);

            // Atomic-per-album: encode + cover go into a staging directory, then we
            // atomically swap with the final location. Any failure before commit only
            // touches the staging dir; an existing prior good encode is untouched.
            let artist_dir = album_info.artist.replace(" ", "_");
            let album_name = album_info.name.replace(" ", "_");
            let album_dir_name = format!("{} - {} - {}", album_id, artist_dir, album_name);
            let albums_root = output_media.join("albums");
            let final_album_dir = albums_root.join(&album_dir_name);
            let staging_album_dir = albums_root.join(format!("{}.tmp", album_dir_name));

            let result = run_album(
                &album_info,
                &album_id,
                &profiles,
                segment_duration_seconds,
                &output_media,
                &metadata_dir,
                &final_album_dir,
                &staging_album_dir,
                job_semaphore.clone(),
                metadata_lock.clone(),
            )
            .await;

            if let Err(e) = &result {
                tracing::error!(
                    "{} Error processing album: {}",
                    album_log_prefix(&album_id, &album_info),
                    e
                );
                // Best-effort cleanup of the staging dir if it still exists. After a
                // successful media commit (staging→final rename) the staging path is
                // gone and this is a no-op; after a failed YAML write `run_album`
                // already rolled back the final dir.
                let _ = std::fs::remove_dir_all(&staging_album_dir);
            }
            result?;

            let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            tracing::info!(
                "[{}/{}] {} Completed",
                done,
                total,
                album_log_prefix(&album_id, &album_info)
            );
            Result::<()>::Ok(())
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        task.await??;
    }

    tracing::info!("Generate complete");
    Ok(())
}

/// Copy cover image from source into `album_dir`.
fn copy_cover_image(album: &scan::AlbumInfo, album_dir: &Path) -> Result<()> {
    if !album.cover_path.exists() {
        return Err(anyhow::anyhow!(
            "Cover image not found at {}",
            album.cover_path.display()
        ));
    }

    std::fs::create_dir_all(album_dir)?;

    let cover_filename = album
        .cover_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid cover filename"))?;

    let target_path = album_dir.join(cover_filename);
    std::fs::copy(&album.cover_path, &target_path)?;

    Ok(())
}

/// Per-album atomic worker.
///
/// All on-disk effects for this album are confined to either:
/// - `staging_album_dir` (during encode); removed on failure, renamed to `final_album_dir`
///   on success.
/// - the metadata YAML for this album_id; only written *after* the media commit.
///
/// If the metadata write fails after the media commit, we roll the media dir back to its
/// pre-run absence (we cannot restore a prior good encode that was replaced; re-run to
/// regenerate). The function never leaves a half-encoded album dir behind.
#[allow(clippy::too_many_arguments)]
async fn run_album(
    album_info: &scan::AlbumInfo,
    album_id: &str,
    profiles: &[config::AudioProfile],
    segment_duration_seconds: f64,
    output_media: &Path,
    metadata_dir: &Path,
    final_album_dir: &Path,
    staging_album_dir: &Path,
    job_semaphore: std::sync::Arc<tokio::sync::Semaphore>,
    metadata_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
) -> Result<()> {
    // Ensure a clean staging directory. A leftover from a previous aborted run would
    // otherwise pollute this encode.
    if staging_album_dir.exists() {
        std::fs::remove_dir_all(staging_album_dir).with_context_path(staging_album_dir)?;
    }

    let processor = AudioProcessor::new(segment_duration_seconds, profiles.to_vec());
    processor
        .process_album(album_info, staging_album_dir, album_id, job_semaphore)
        .await?;

    copy_cover_image(album_info, staging_album_dir)?;

    // ---- Media commit: atomic swap of staging into the final album path. ----
    // If a prior final dir exists (re-encode) we remove it first; rename replaces nothing
    // atomically when target exists for directories, so this is the unavoidable window.
    if let Some(parent) = final_album_dir.parent() {
        std::fs::create_dir_all(parent).with_context_path(parent)?;
    }
    if final_album_dir.exists() {
        std::fs::remove_dir_all(final_album_dir).with_context_path(final_album_dir)?;
    }
    std::fs::rename(staging_album_dir, final_album_dir).map_err(|e| {
        anyhow::anyhow!(
            "Failed to commit album {} -> {}: {}",
            staging_album_dir.display(),
            final_album_dir.display(),
            e
        )
    })?;

    // ---- Metadata commit. If this fails, roll the media commit back so the on-disk
    // state remains atomic-per-album (either fully present or fully absent). ----
    let _guard = metadata_lock.lock().await;
    if let Err(e) = db::importer::import_album(
        metadata_dir,
        output_media,
        profiles,
        album_info.clone(),
        Some(album_id.to_string()),
    )
    .await
    {
        let _ = std::fs::remove_dir_all(final_album_dir);
        return Err(e);
    }

    Ok(())
}

/// Small extension to attach a path to an io error for better context.
trait WithContextPath<T> {
    fn with_context_path(self, path: &Path) -> Result<T>;
}

impl<T> WithContextPath<T> for std::io::Result<T> {
    fn with_context_path(self, path: &Path) -> Result<T> {
        self.with_context(|| path.display().to_string())
    }
}
