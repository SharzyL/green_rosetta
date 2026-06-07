/**
 * API Configuration
 * Uses same-origin relative URLs.
 *
 * - Dev: Vite proxy forwards /api, /stream, /media to the backend.
 * - Prod: serve frontend and backend from the same origin (or via reverse proxy).
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
};

export default {
  API_BASE_URL,
  API_ENDPOINTS,
};
