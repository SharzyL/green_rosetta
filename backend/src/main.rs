mod api;
mod auth;
mod config;
mod db;
mod scheduler;
mod streaming;

use anyhow::Result;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use clap::Parser;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tower::limit::ConcurrencyLimitLayer;
use tower::service_fn;
use tower_http::cors::CorsLayer;
use tower_http::cors::{AllowOrigin, Any};
use tower_http::services::ServeDir;

#[derive(Parser, Debug)]
#[command(name = "Green Rosetta Backend")]
#[command(about = "Music radio streaming service with DASH/CMAF", long_about = None)]
struct Args {
    /// Listen address (e.g., 127.0.0.1:8080)
    #[arg(short, long)]
    listen: Option<String>,

    /// Enable debug logging (overrides RUST_LOG)
    #[arg(long)]
    debug: bool,

    /// Configuration file path
    #[arg(short, long, default_value = "./backend.config.toml")]
    config: String,

    /// Allow socket address reuse (SO_REUSEADDR)
    #[arg(long)]
    reuse_addr: bool,

    /// Serve frontend static content from this directory (SPA fallback to index.html)
    #[arg(long, value_name = "DIR")]
    web_dir: Option<PathBuf>,

    /// Print a bcrypt password hash to stdout, then exit.
    ///
    /// The password is read from stdin (you can type it and press Enter, or pipe it in).
    #[arg(long)]
    hash_password: bool,
}

#[derive(Clone)]
pub(crate) struct AppState {
    config: Arc<config::Config>,
    db: Arc<tokio::sync::RwLock<db::Database>>,
    scheduler: Arc<scheduler::Scheduler>,
    admin_rate_limiter: Arc<tokio::sync::Mutex<AdminRateLimiter>>,
    admin_sessions: Arc<tokio::sync::RwLock<auth::AdminSessions>>,
    /// Serializes admin metadata reloads so two concurrent reloads can't run at once.
    reload_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug)]
struct AdminRateLimiter {
    window_start: Instant,
    count: u64,
}

async fn health_check() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}

async fn scheduler_advance_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    if let Err(e) = state
        .scheduler
        .maintain(
            state.config.streaming.time_shift_buffer_depth,
            state.config.streaming.min_future_manifest_duration,
        )
        .await
    {
        tracing::error!("Failed to maintain scheduler: {}", e);
    }
    next.run(req).await
}

async fn cache_headers_middleware(
    _state: State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let res = next.run(req).await;

    // Only attach caching headers for successful responses (including partial content).
    let status = res.status();
    let ok = status.is_success() || status == StatusCode::PARTIAL_CONTENT;
    if !ok {
        return res;
    }

    let has_cache_control = res.headers().contains_key(header::CACHE_CONTROL);

    let mut res = res;

    // Helper to overwrite Cache-Control.
    let set_cc = |res: &mut Response, value: &'static str| {
        res.headers_mut()
            .insert(header::CACHE_CONTROL, value.parse().unwrap());
    };

    // Explicit policies by route group.
    if path == "/stream/manifest.mpd" {
        // Dynamic MPD: always revalidate.
        set_cc(&mut res, "no-cache");
        return res;
    }

    if path.starts_with("/api/utc") || path.starts_with("/api/sync/time") {
        // These handlers already set Cache-Control: no-store, but keep a safe fallback.
        if !has_cache_control {
            set_cc(&mut res, "no-store");
        }
        return res;
    }

    if path.starts_with("/api/admin/") {
        // Authenticated + mutable state.
        set_cc(&mut res, "no-store");
        return res;
    }

    if path.starts_with("/api/tracks/") || path.starts_with("/api/albums/") {
        // Metadata is stable enough to cache briefly.
        if !has_cache_control {
            set_cc(&mut res, "public, max-age=300");
        }
        return res;
    }

    if path.starts_with("/media/") {
        // Static files (segments/covers) are content-addressed by path; cache aggressively.
        if !has_cache_control {
            set_cc(&mut res, "public, max-age=31536000, immutable");
        }
        return res;
    }

    // Frontend static assets (when running with --web-dir).
    if path.starts_with("/assets/") {
        if !has_cache_control {
            set_cc(&mut res, "public, max-age=31536000, immutable");
        }
        return res;
    }

    // HTML entrypoints can be cached briefly, but should revalidate frequently so deployments
    // pick up changes without long stale windows.
    let is_html_route = path == "/" || path.ends_with('/') || path.ends_with(".html");
    if is_html_route {
        if !has_cache_control {
            set_cc(&mut res, "public, max-age=60, must-revalidate");
        }
        return res;
    }

    res
}

async fn admin_auth_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let token = match auth::parse_cookie(req.headers(), auth::ADMIN_SESSION_COOKIE) {
        Some(t) => t,
        None => return (StatusCode::UNAUTHORIZED, "Not logged in").into_response(),
    };

    let ok = {
        let mut sessions = state.admin_sessions.write().await;
        sessions.validate(&token)
    };
    if !ok {
        return (StatusCode::UNAUTHORIZED, "Not logged in").into_response();
    }

    next.run(req).await
}

async fn admin_rate_limit_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    // Process-wide fixed-window rate limiter for admin endpoints.
    // This is intentionally simple and cheap: it protects login/password verification paths.
    let limit = state.config.admin.rate_limit_per_second.max(1);
    {
        let mut rl = state.admin_rate_limiter.lock().await;
        let now = Instant::now();
        if now.duration_since(rl.window_start).as_secs() >= 1 {
            rl.window_start = now;
            rl.count = 0;
        }
        if rl.count >= limit {
            return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
        }
        rl.count += 1;
    }

    next.run(req).await
}

async fn manifest(
    State(state): State<AppState>,
) -> Result<(HeaderMap, String), (StatusCode, String)> {
    let periods = state
        .scheduler
        .get_mpd_periods(
            state.config.streaming.time_shift_buffer_depth,
            state.config.streaming.min_future_manifest_duration,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let media_base_url = if state.config.storage.backend == "s3" {
        let domain = state
            .config
            .storage
            .s3
            .as_ref()
            .and_then(|s| s.public_access_domain.as_deref())
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage.s3.public_access_domain must be set for S3 mode".to_string(),
                )
            })?;
        format!("{}/media", domain.trim_end_matches('/'))
    } else {
        "/media".to_string()
    };

    let generator = streaming::DashGenerator::new(media_base_url, &state.config.streaming);

    let mpd = generator
        .generate_mpd(state.scheduler.mpd_start_time(), periods)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/dash+xml".parse().unwrap());
    // Dynamic MPD: avoid stale manifests through proxies.
    headers.insert("Cache-Control", "no-cache".parse().unwrap());

    Ok((headers, mpd))
}

/// Load the metadata database from the configured storage backend (local dir or S3 bucket).
///
/// Used both at startup and by the admin reload endpoint. Parse errors (e.g. malformed YAML)
/// propagate as `Err`, so callers fail-fast and can keep serving the previous database.
pub(crate) async fn load_database(config: &config::Config) -> Result<db::Database> {
    if config.storage.backend == "s3" {
        let s3_cfg = config.storage.s3.as_ref().ok_or_else(|| {
            anyhow::anyhow!("storage.backend is 's3' but storage.s3 is not configured")
        })?;
        db::s3_loader::load_database_from_s3(s3_cfg).await
    } else {
        let root = config
            .storage
            .base_path
            .clone()
            .unwrap_or_else(|| ".".to_string());
        let metadata_dir = std::path::PathBuf::from(root).join("metadata");
        db::Database::load(&metadata_dir)
    }
}

fn apply_env_overrides(config: &mut config::Config) -> Result<()> {
    // Ensure `public_access_domain` is an absolute URL so it can be embedded in MPDs and redirects.
    fn normalize_public_access_domain(raw: &str) -> Result<String> {
        let d = raw.trim().trim_end_matches('/').to_string();
        if d.is_empty() {
            return Err(anyhow::anyhow!(
                "storage.s3.public_access_domain must be non-empty when using S3 public access"
            ));
        }
        if d.starts_with("http://") || d.starts_with("https://") {
            Ok(d)
        } else {
            Ok(format!("https://{}", d))
        }
    }

    // Convenience: allow S3 credentials via env.
    if let Some(s3) = config.storage.s3.as_mut() {
        if s3
            .access_key_id
            .as_ref()
            .is_none_or(|v| v.trim().is_empty())
            && let Ok(v) = std::env::var("AWS_ACCESS_KEY_ID")
            && !v.trim().is_empty()
        {
            s3.access_key_id = Some(v);
        }
        if s3.secret_access_key.is_none()
            && let Ok(v) = std::env::var("AWS_SECRET_ACCESS_KEY")
            && !v.trim().is_empty()
        {
            s3.secret_access_key = Some(v);
        }
        if let Some(domain) = s3.public_access_domain.as_deref() {
            s3.public_access_domain = Some(normalize_public_access_domain(domain)?);
        }
    }

    // Scrutinizer (admin) credentials can be provided via environment variables for convenience.
    if let Ok(v) = std::env::var("SCRUTINIZER_USERNAME")
        && !v.trim().is_empty()
    {
        config.admin.username = Some(v);
    }
    if let Ok(v) = std::env::var("SCRUTINIZER_PASSWORD_HASH")
        && !v.trim().is_empty()
    {
        config.admin.password_hash = Some(v);
    }

    Ok(())
}

fn validate_admin_config(cfg: &config::Config) -> Result<()> {
    let user_ok = cfg
        .admin
        .username
        .as_deref()
        .map(str::trim)
        .is_some_and(|v| !v.is_empty());
    let pass_ok = cfg
        .admin
        .password_hash
        .as_deref()
        .map(str::trim)
        .is_some_and(|v| !v.is_empty());

    if !user_ok || !pass_ok {
        return Err(anyhow::anyhow!(
            "Admin auth is not configured. Set admin.username and admin.password_hash in backend.config.toml or set SCRUTINIZER_USERNAME and SCRUTINIZER_PASSWORD_HASH env vars."
        ));
    }
    Ok(())
}

async fn resolve_listen_addr(raw: &str) -> Result<SocketAddr> {
    // Accept either:
    // - "127.0.0.1:8080" (SocketAddr)
    // - "localhost:8080" (DNS name)
    // - "[::1]:8080" (IPv6)
    if let Ok(addr) = raw.parse::<SocketAddr>() {
        return Ok(addr);
    }

    let mut addrs = tokio::net::lookup_host(raw)
        .await
        .map_err(|e| anyhow::anyhow!("invalid socket address syntax: {}", e))?
        .collect::<Vec<_>>();

    if addrs.is_empty() {
        return Err(anyhow::anyhow!("invalid socket address syntax"));
    }

    // Prefer IPv4 if available (more consistent in dev setups).
    addrs.sort_by_key(|a| if a.is_ipv4() { 0 } else { 1 });
    Ok(addrs[0])
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    if args.hash_password {
        use std::io::{self, Write};

        eprint!("Enter password: ");
        io::stderr().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let password = input.trim_end_matches(['\n', '\r']).to_string();
        if password.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty password; nothing to hash"));
        }

        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;
        println!("{hash}");
        return Ok(());
    }

    // Load configuration
    let config_path = std::path::PathBuf::from(&args.config);
    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Config not found at {}. Create one from docs/backend.config.toml (example) or pass --config PATH.",
            config_path.display()
        ));
    }

    let mut config = config::Config::load(&config_path)?;

    apply_env_overrides(&mut config)?;
    validate_admin_config(&config)?;

    // Initialize tracing with environment variable support.
    // Priority:
    // 1) `--debug`
    // 2) `RUST_LOG`
    // 3) `logging.level` from config
    // 4) default "info"
    let filter = if args.debug {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            let level = config.logging.level.trim();
            if level.is_empty() {
                tracing_subscriber::EnvFilter::new("info")
            } else {
                tracing_subscriber::EnvFilter::new(level)
            }
        })
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    if config.streaming.suggested_presentation_delay <= 0.0 {
        tracing::warn!(
            "streaming.suggested_presentation_delay is <= 0 ({}); live playback may start too close to the live edge and stall at startup",
            config.streaming.suggested_presentation_delay
        );
    }
    if config.streaming.time_shift_buffer_depth <= config.streaming.suggested_presentation_delay {
        tracing::warn!(
            "streaming.time_shift_buffer_depth ({}) should be > streaming.suggested_presentation_delay ({}) to allow safe startup behind the live edge",
            config.streaming.time_shift_buffer_depth,
            config.streaming.suggested_presentation_delay
        );
    }
    if config.streaming.min_future_manifest_duration <= 0.0 {
        tracing::warn!(
            "streaming.min_future_manifest_duration is <= 0 ({}); MPD may not include enough future timeline for smooth transitions",
            config.streaming.min_future_manifest_duration
        );
    }

    // Load database (local dir or S3 bucket, depending on storage backend).
    let db_loaded = load_database(&config).await?;

    tracing::info!(
        "Loaded {} albums with {} total tracks",
        db_loaded.albums.len(),
        db_loaded.total_tracks()
    );

    // Determine listen address (CLI overrides config)
    let addr = if let Some(listen_addr) = args.listen {
        resolve_listen_addr(&listen_addr).await?
    } else {
        let host = config
            .server
            .host
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .ok_or_else(|| anyhow::anyhow!("server.host is not set; pass --listen host:port"))?;
        let port = config
            .server
            .port
            .ok_or_else(|| anyhow::anyhow!("server.port is not set; pass --listen host:port"))?;
        let port = if port == 0 {
            return Err(anyhow::anyhow!("server.port is 0; pass --listen host:port"));
        } else {
            port
        };

        SocketAddr::from((host.parse::<std::net::IpAddr>()?, port))
    };

    // Initialize scheduler.
    // MPD availabilityStartTime is fixed at init; the scheduler maintains a sliding window
    // [t_now - TSBD, t_now + min_future_manifest_duration].
    let db = Arc::new(tokio::sync::RwLock::new(db_loaded));
    let scheduler =
        scheduler::Scheduler::new(db.clone(), config.streaming.suggested_presentation_delay)
            .await?;

    let state = AppState {
        config: Arc::new(config),
        db: db.clone(),
        scheduler: Arc::new(scheduler),
        admin_rate_limiter: Arc::new(tokio::sync::Mutex::new(AdminRateLimiter {
            window_start: Instant::now(),
            count: 0,
        })),
        admin_sessions: Arc::new(tokio::sync::RwLock::new(auth::AdminSessions::default())),
        reload_lock: Arc::new(tokio::sync::Mutex::new(())),
    };

    // Build router with CORS support
    let middleware_state = state.clone();
    let mut app = Router::new()
        .route("/health", get(health_check))
        .route("/stream/manifest.mpd", get(manifest))
        .route("/api/tracks/{track_id}", get(api::track_info))
        .route("/api/albums/{album_id}", get(api::album))
        .route("/api/utc", get(api::utc_time))
        .route("/api/sync/time", get(api::sync_time));

    // Admin routes: cookie-based auth + basic rate/concurrency limiting.
    let admin_state = middleware_state.clone();
    let admin_public = Router::new()
        .route("/login", post(api::admin_login))
        .route("/logout", post(api::admin_logout));

    let admin_protected = Router::new()
        .route("/me", get(api::admin_me))
        .route("/albums", get(api::admin_albums))
        .route(
            "/albums/{album_id}/enabled",
            put(api::admin_set_album_enabled),
        )
        .route("/reload", post(api::admin_reload))
        .route_layer(from_fn_with_state(admin_state, admin_auth_middleware));

    let admin_routes = admin_public
        .merge(admin_protected)
        .route_layer(from_fn_with_state(
            middleware_state.clone(),
            admin_rate_limit_middleware,
        ))
        .layer(ConcurrencyLimitLayer::new(
            state.config.admin.max_concurrency,
        ));

    app = app.nest("/api/admin", admin_routes);

    // No backward-compat for segment routes in S3 public-domain mode: clients should fetch
    // segments directly from the public domain URLs embedded in the MPD.
    if state.config.storage.backend == "local" {
        let base_root = state
            .config
            .storage
            .base_path
            .clone()
            .unwrap_or_else(|| ".".to_string());
        let media_dir = std::path::PathBuf::from(base_root).join("media");
        let media_service = ServeDir::new(media_dir);

        app = app
            // Serve static assets from the configured local media directory.
            .nest_service("/media", media_service);
    }

    if let Some(web_dir) = &args.web_dir {
        tracing::info!("Serving frontend from {}", web_dir.display());
        app = app.fallback_service(ServeDir::new(web_dir).not_found_service(service_fn(
            |req: axum::http::Request<axum::body::Body>| async move {
                // Redirect unknown browser navigations to the dedicated 404 page entrypoint.
                // Keep non-HTML (e.g. fetch) requests as plain 404s.
                let path = req.uri().path();
                if path == "/404" {
                    return Ok::<_, Infallible>(
                        axum::response::Redirect::temporary("/404/").into_response(),
                    );
                }
                if path.starts_with("/404/") {
                    return Ok::<_, Infallible>(
                        (StatusCode::NOT_FOUND, "Not Found").into_response(),
                    );
                }

                let wants_html = req
                    .headers()
                    .get(axum::http::header::ACCEPT)
                    .and_then(|h| h.to_str().ok())
                    .is_some_and(|v| v.contains("text/html") || v.contains("*/*"));

                if req.method() == axum::http::Method::GET && wants_html {
                    Ok::<_, Infallible>(
                        axum::response::Redirect::temporary("/404/").into_response(),
                    )
                } else {
                    Ok::<_, Infallible>((StatusCode::NOT_FOUND, "Not Found").into_response())
                }
            },
        )));
    }

    // CORS is disabled by default (same-origin only). Enable explicitly via config.
    let app = if state.config.cors.allowed_origins.is_empty() {
        app
    } else {
        let origins = &state.config.cors.allowed_origins;
        let cors = if origins.iter().any(|o| o.trim() == "*") {
            CorsLayer::new().allow_origin(Any)
        } else {
            let mut parsed = Vec::new();
            for o in origins {
                if o.trim().is_empty() {
                    continue;
                }
                match o.parse() {
                    Ok(v) => parsed.push(v),
                    Err(_) => tracing::warn!("Invalid CORS origin in config: '{}'", o),
                }
            }
            CorsLayer::new().allow_origin(AllowOrigin::list(parsed))
        }
        .allow_methods([axum::http::Method::GET, axum::http::Method::PUT])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ]);

        app.layer(cors)
    }
    .layer(from_fn_with_state(
        middleware_state.clone(),
        cache_headers_middleware,
    ))
    .layer(from_fn_with_state(
        middleware_state,
        scheduler_advance_middleware,
    ))
    .with_state(state);

    // Run server
    tracing::info!("Server listening on {}", addr);

    let listener = if args.reuse_addr {
        // Create socket with SO_REUSEADDR and SO_REUSEPORT enabled
        let socket = socket2::Socket::new(
            if addr.is_ipv6() {
                socket2::Domain::IPV6
            } else {
                socket2::Domain::IPV4
            },
            socket2::Type::STREAM,
            None,
        )?;
        socket.set_reuse_address(true)?;

        // Also set SO_REUSEPORT for better reuse behavior
        #[cfg(unix)]
        socket.set_reuse_port(true)?;

        tracing::info!("Binding with SO_REUSEADDR enabled");
        socket.bind(&socket2::SockAddr::from(addr))?;
        socket.listen(1024)?;

        // Convert socket2::Socket to std::net::TcpListener, then to tokio
        let std_listener: std::net::TcpListener = socket.into();
        std_listener.set_nonblocking(true)?;
        tokio::net::TcpListener::from_std(std_listener)?
    } else {
        tokio::net::TcpListener::bind(addr).await?
    };

    axum::serve(listener, app).await?;

    Ok(())
}
