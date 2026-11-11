/**
 * API Configuration
 * Uses same-origin relative URLs.
 *
 * - Dev: Vite proxy forwards /api and /stream to the worker (`pnpm dev`).
 * - Prod: the worker serves the frontend and the API from the same origin.
 */

export const API_BASE_URL = "";

export const API_ENDPOINTS = {
  album: (albumId: string) =>
    `${API_BASE_URL}/api/albums/${encodeURIComponent(albumId)}`,
  track: (trackId: string) =>
    `${API_BASE_URL}/api/tracks/${encodeURIComponent(trackId)}`,
  admin_login: () => `${API_BASE_URL}/api/admin/login`,
  admin_logout: () => `${API_BASE_URL}/api/admin/logout`,
  admin_me: () => `${API_BASE_URL}/api/admin/me`,
  admin_albums: () => `${API_BASE_URL}/api/admin/albums`,
  admin_album_enabled: (albumId: string) =>
    `${API_BASE_URL}/api/admin/albums/${encodeURIComponent(albumId)}/enabled`,
  admin_reload: () => `${API_BASE_URL}/api/admin/reload`,
  manifest: () => `${API_BASE_URL}/stream/manifest.mpd`,
  /** Without a position the station answers for its live edge. */
  now: (presentationSeconds?: number) =>
    presentationSeconds === undefined
      ? `${API_BASE_URL}/api/now`
      : `${API_BASE_URL}/api/now?t=${encodeURIComponent(presentationSeconds.toFixed(3))}`,
};

export default {
  API_BASE_URL,
  API_ENDPOINTS,
};
