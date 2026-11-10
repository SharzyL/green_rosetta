// JSON bodies exchanged between the worker and the frontend.

/** `GET /api/tracks/{track_id}` */
export interface TrackInfoResponse {
  album_id: string;
  album_name: string;
  album_artist: string;
  cover_url: string;
  track_id: string;
  track_title: string;
  track_artist: string | null;
  track_number: number;
  disc_number: number;
  /** Encoded duration, matching the track's MPD Period. */
  duration_seconds: number;
}

export interface AlbumTrackResponse {
  track_id: string;
  track_title: string;
  track_artist: string | null;
  track_number: number;
  disc_number: number;
  /** Source length of the track. */
  duration_seconds: number;
}

/** `GET /api/albums/{album_id}` */
export interface AlbumResponse {
  album_id: string;
  album_name: string;
  album_artist: string;
  release_date: string | null;
  original_release_date: string | null;
  album_label: string | null;
  album_media: string | null;
  album_catalog_number: string | null;
  musicbrainz_album_id: string | null;
  disc_count: number;
  cover_url: string;
  tracks: AlbumTrackResponse[];
}

/** `GET /api/now?t=<seconds>`: what is on air at a point on the presentation timeline. */
export interface NowResponse {
  track_id: string;
  album_id: string;
  period_start_seconds: number;
  duration_seconds: number;
}

/** `GET /api/sync/time` */
export interface TimeSyncResponse {
  timestamp: string;
  unix_millis: number;
}

/** Whether an album is currently on air or queued ahead in the continuous stream. */
export type SchedulingState = "playing" | "queued";

/** An element of `GET /api/admin/albums`, and the body of `PUT /api/admin/albums/{id}/enabled`. */
export interface AdminAlbumSummaryResponse {
  album_id: string;
  album_name: string;
  album_artist: string;
  enabled: boolean;
  track_count: number;
  cover_url: string;
  /** Absent when the album is neither on air nor queued. */
  scheduling_state?: SchedulingState;
}

/** Body of `PUT /api/admin/albums/{id}/enabled`. */
export interface SetAlbumEnabledRequest {
  enabled: boolean;
}

/** `POST /api/admin/reload` */
export interface ReloadSummary {
  albums_before: number;
  albums_after: number;
  added: number;
  removed: number;
  kept_disabled: number;
}

/** Body of `POST /api/admin/login`. */
export interface AdminLoginRequest {
  username: string;
  password: string;
}

/** `POST /api/admin/login`, `POST /api/admin/logout`, `GET /api/admin/me` */
export interface AdminOkResponse {
  ok: boolean;
}
