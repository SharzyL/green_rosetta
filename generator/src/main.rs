use anyhow::Result;
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
    tracing_subscriber::fmt().with_env_filter(filter).init();

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
        println!("Generating albums by scanning directories:");
    } else {
        println!("Generating albums from album directories:");
    }
    for i in &album_dirs {
        println!("  - {}", i.display());
    }
    println!("Output dir: {}", output_dir.display());
    println!("Media dir: {}", output_media.display());
    println!("Metadata dir: {}", metadata_dir.display());
    println!("Parallel jobs: {}", jobs);

    if let Some(ref f) = filter {
        println!("Filter: {}", f);
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
        println!(
            "Filtered to {} albums (from {})",
            albums.len(),
            original_count
        );
    } else {
        println!("Found {} albums", albums.len());
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

    for (album_info, album_id) in albums.into_iter().zip(album_ids.into_iter()) {
        let output_media = output_media.clone();
        let metadata_dir = metadata_dir.clone();
        let completed = completed.clone();
        let profiles = profiles.clone();
        let metadata_lock = metadata_lock.clone();
        let job_semaphore = job_semaphore.clone();

        let task = tokio::spawn(async move {
            println!(
                "\n{} Processing: {} by {}",
                album_log_prefix(&album_id, &album_info),
                album_info.name,
                album_info.artist
            );
            warn_album_tag_issues(&album_id, &album_info);

            let processor = AudioProcessor::new(segment_duration_seconds, profiles.clone());
            if let Err(e) = processor
                .process_album(
                    &album_info,
                    &output_media,
                    &album_id,
                    job_semaphore.clone(),
                )
                .await
            {
                eprintln!(
                    "{} Error processing album: {}",
                    album_log_prefix(&album_id, &album_info),
                    e
                );
                return Err(e);
            }

            // Copy cover image
            if let Err(e) = copy_cover_image(&album_info, &output_media, &album_id) {
                eprintln!(
                    "{} Error copying cover: {}",
                    album_log_prefix(&album_id, &album_info),
                    e
                );
                return Err(e);
            }

            // Avoid concurrent writers touching the same metadata directory.
            // (Encoding is parallel; metadata updates are quick and serialized.)
            let _guard = metadata_lock.lock().await;
            db::importer::import_album(
                &metadata_dir,
                &output_media,
                &profiles,
                album_info.clone(),
                Some(album_id.clone()),
            )
            .await?;

            let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            println!(
                "[{}/{}] {} Completed",
                done,
                total,
                album_log_prefix(&album_id, &album_info)
            );
            Ok(())
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        task.await??;
    }

    println!("\n✓ Generate complete");
    Ok(())
}

/// Copy cover image from source to album directory in output storage
fn copy_cover_image(album: &scan::AlbumInfo, output: &Path, album_id: &str) -> Result<()> {
    if !album.cover_path.exists() {
        return Err(anyhow::anyhow!(
            "Cover image not found at {}",
            album.cover_path.display()
        ));
    }

    // Build target directory: output/albums/{album_id} - {artist} - {album_name}
    let artist_dir = album.artist.replace(" ", "_");
    let album_name = album.name.replace(" ", "_");
    let album_dir = output.join(format!(
        "albums/{} - {} - {}",
        album_id, artist_dir, album_name
    ));

    // Create album directory if it doesn't exist
    std::fs::create_dir_all(&album_dir)?;

    // Copy cover image with original filename
    let cover_filename = album
        .cover_path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid cover filename"))?;

    let target_path = album_dir.join(cover_filename);
    std::fs::copy(&album.cover_path, &target_path)?;

    Ok(())
}
