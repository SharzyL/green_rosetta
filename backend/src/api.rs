use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{}/{}", base, path)
}

fn media_base_url(state: &AppState) -> Result<String, (StatusCode, String)> {
    if state.config.storage.backend == "s3" {
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
        Ok(format!("{}/media", domain.trim_end_matches('/')))
    } else {
        Ok("/media".to_string())
    }
}

#[derive(Serialize)]
pub(crate) struct TrackInfoResponse {
    pub album_id: String,
    pub album_name: String,
    pub album_artist: String,
    pub cover_url: String,
    pub track_id: String,
    pub track_title: String,
    pub track_artist: Option<String>,
    pub track_number: u32,
    pub disc_number: u32,
    pub duration_seconds: f64,
}

pub(crate) async fn track_info(
    Path(track_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TrackInfoResponse>, (StatusCode, String)> {
    let media_base = media_base_url(&state)?;
    let (album, track) = state
        .db
        .read()
        .await
        .get_track(&track_id)
        .map(|(a, t)| (a.clone(), t.clone()))
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Track not found".to_string()))?;

    let duration_seconds = track.encoded_duration_seconds();

    Ok(Json(TrackInfoResponse {
        album_id: album.id,
        album_name: album.name,
        album_artist: album.artist,
        cover_url: join_url(&media_base, &album.cover_path),
        track_id: track.id,
        track_title: track.title,
        track_artist: track.artist.clone(),
        track_number: track.track_number,
        disc_number: track.disc_number,
        duration_seconds,
    }))
}

#[derive(Serialize)]
pub(crate) struct AlbumTrackResponse {
    pub track_id: String,
    pub track_title: String,
    pub track_artist: Option<String>,
    pub track_number: u32,
    pub disc_number: u32,
    pub duration_seconds: f64,
}

#[derive(Serialize)]
pub(crate) struct AlbumResponse {
    pub album_id: String,
    pub album_name: String,
    pub album_artist: String,
    pub release_date: Option<String>,
    pub original_release_date: Option<String>,
    pub album_label: Option<String>,
    pub album_media: Option<String>,
    pub album_catalog_number: Option<String>,
    pub musicbrainz_album_id: Option<String>,
    pub disc_count: u32,
    pub cover_url: String,
    pub tracks: Vec<AlbumTrackResponse>,
}

pub(crate) async fn album(
    Path(album_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<AlbumResponse>, (StatusCode, String)> {
    let media_base = media_base_url(&state)?;
    let album = state.db.read().await.get_album(&album_id).cloned().ok_or_else(|| {
        tracing::error!("Album not found: {}", album_id);
        (StatusCode::NOT_FOUND, "Album not found".to_string())
    })?;

    let mut tracks = Vec::with_capacity(album.tracks.len());
    for track in &album.tracks {
        tracks.push(AlbumTrackResponse {
            track_id: track.id.clone(),
            track_title: track.title.clone(),
            track_artist: track.artist.clone(),
            track_number: track.track_number,
            disc_number: track.disc_number,
            duration_seconds: track.length_seconds,
        });
    }

    Ok(Json(AlbumResponse {
        album_id: album.id.clone(),
        album_name: album.name.clone(),
        album_artist: album.artist.clone(),
        release_date: album.release_date.clone(),
        original_release_date: album.original_release_date.clone(),
        album_label: album.label.clone(),
        album_media: album.media.clone(),
        album_catalog_number: album.catalog_number.clone(),
        musicbrainz_album_id: album.musicbrainz_album_id.clone(),
        disc_count: album.disc_count,
        cover_url: join_url(&media_base, &album.cover_path),
        tracks,
    }))
}

#[derive(Serialize)]
pub(crate) struct AdminAlbumSummaryResponse {
    pub album_id: String,
    pub album_name: String,
    pub album_artist: String,
    pub enabled: bool,
    pub track_count: usize,
    pub cover_url: String,
}

pub(crate) async fn admin_albums(
    State(state): State<AppState>,
) -> Result<Json<Vec<AdminAlbumSummaryResponse>>, (StatusCode, String)> {
    let media_base = media_base_url(&state)?;
    let db = state.db.read().await;
    let mut albums: Vec<AdminAlbumSummaryResponse> = db
        .albums
        .iter()
        .map(|a| AdminAlbumSummaryResponse {
            album_id: a.id.clone(),
            album_name: a.name.clone(),
            album_artist: a.artist.clone(),
            enabled: a.enabled,
            track_count: a.tracks.len(),
            cover_url: join_url(&media_base, &a.cover_path),
        })
        .collect();
    albums.sort_by(|a, b| {
        a.album_artist
            .cmp(&b.album_artist)
            .then_with(|| a.album_name.cmp(&b.album_name))
    });
    Ok(Json(albums))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetAlbumEnabledRequest {
    pub enabled: bool,
}

pub(crate) async fn admin_set_album_enabled(
    Path(album_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SetAlbumEnabledRequest>,
) -> Result<Json<AdminAlbumSummaryResponse>, (StatusCode, String)> {
    let media_base = media_base_url(&state)?;
    // We intentionally keep enabled state in-memory only for now.
    // This avoids write contention/corruption in the metadata directory.
    let mut db = state.db.write().await;
    let target = db
        .albums
        .iter_mut()
        .find(|a| a.id == album_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Album not found".to_string()))?;

    target.enabled = req.enabled;

    Ok(Json(AdminAlbumSummaryResponse {
        album_id: target.id.clone(),
        album_name: target.name.clone(),
        album_artist: target.artist.clone(),
        enabled: target.enabled,
        track_count: target.tracks.len(),
        cover_url: join_url(&media_base, &target.cover_path),
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub(crate) struct AdminLoginResponse {
    pub ok: bool,
}

pub(crate) async fn admin_login(
    State(state): State<AppState>,
    Json(req): Json<AdminLoginRequest>,
) -> Result<(HeaderMap, Json<AdminLoginResponse>), (StatusCode, String)> {
    let ok = crate::auth::verify_admin_password(&state.config, &req.username, &req.password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !ok {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let token = crate::auth::new_session_token();
    let expires_at = std::time::Instant::now() + crate::auth::session_ttl(&state.config);

    {
        let mut sessions = state.admin_sessions.write().await;
        sessions.insert(crate::auth::AdminSession {
            token: token.clone(),
            expires_at,
        });
    }

    let set_cookie = crate::auth::build_set_cookie(&token, &state.config)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, set_cookie.parse().unwrap());
    Ok((headers, Json(AdminLoginResponse { ok: true })))
}

#[derive(Serialize)]
pub(crate) struct AdminMeResponse {
    pub ok: bool,
}

pub(crate) async fn admin_me(
    State(_state): State<AppState>,
) -> Result<Json<AdminMeResponse>, (StatusCode, String)> {
    // This endpoint sits behind the admin auth middleware. If you can reach it, you're logged in.
    Ok(Json(AdminMeResponse { ok: true }))
}

pub(crate) async fn admin_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<AdminLoginResponse>), (StatusCode, String)> {
    if let Some(token) = crate::auth::parse_cookie(&headers, crate::auth::ADMIN_SESSION_COOKIE) {
        let mut sessions = state.admin_sessions.write().await;
        sessions.remove(&token);
    }

    let clear_cookie = crate::auth::build_clear_cookie(&state.config)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut out_headers = HeaderMap::new();
    out_headers.insert(header::SET_COOKIE, clear_cookie.parse().unwrap());
    Ok((out_headers, Json(AdminLoginResponse { ok: true })))
}

#[derive(Serialize)]
pub(crate) struct TimeSync {
    pub timestamp: String,
    pub unix_millis: i64,
}

pub(crate) async fn sync_time() -> Json<TimeSync> {
    let now = chrono::Utc::now();
    Json(TimeSync {
        timestamp: now.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string(),
        unix_millis: now.timestamp_millis(),
    })
}

pub(crate) async fn utc_time() -> (HeaderMap, String) {
    // DASH UTCTiming expects a plain-text xs:dateTime body.
    let now = chrono::Utc::now();
    let body = now.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string();

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "text/plain; charset=utf-8".parse().unwrap());
    headers.insert("Cache-Control", "no-store".parse().unwrap());
    (headers, body)
}
