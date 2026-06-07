import {
  type ReactElement,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import "./App.css";
import "./Scrutinizer.css";
import { API_ENDPOINTS } from "./config";

interface AdminAlbum {
  album_id: string;
  album_name: string;
  album_artist: string;
  enabled: boolean;
  track_count: number;
  cover_url: string;
  scheduling_state?: "playing" | "queued" | null;
}

interface ReloadSummary {
  albums_before: number;
  albums_after: number;
  added: number;
  removed: number;
  kept_disabled: number;
}

const Scrutinizer = (): ReactElement => {
  const [loggedIn, setLoggedIn] = useState<boolean>(false);
  const [loginUsername, setLoginUsername] = useState<string>("admin");
  const [loginPassword, setLoginPassword] = useState<string>("");
  const [loginLoading, setLoginLoading] = useState<boolean>(false);
  const [albums, setAlbums] = useState<AdminAlbum[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [savingIds, setSavingIds] = useState<Set<string>>(() => new Set());
  const [reloading, setReloading] = useState<boolean>(false);
  const [notice, setNotice] = useState<string | null>(null);
  // Albums newly discovered by the most recent reload, highlighted until the next manual refresh.
  const [newAlbumIds, setNewAlbumIds] = useState<Set<string>>(() => new Set());

  const checkSession = useCallback(async () => {
    try {
      const res = await fetch(API_ENDPOINTS.admin_me(), {
        credentials: "include",
      });
      if (res.ok) {
        setLoggedIn(true);
        return true;
      }
      if (res.status === 401) {
        setLoggedIn(false);
        return false;
      }
      const msg = await res.text();
      throw new Error(`Admin session error: ${res.status} ${msg}`);
    } catch (e) {
      console.error("Failed to check admin session:", e);
      setError(e instanceof Error ? e.message : "Unknown error");
      setLoggedIn(false);
      return false;
    }
  }, []);

  const fetchAlbums = useCallback(async (): Promise<AdminAlbum[] | null> => {
    setLoading(true);
    setError(null);
    // A fresh listing clears any "new album" highlight from a previous reload.
    setNewAlbumIds(new Set());
    try {
      const res = await fetch(API_ENDPOINTS.admin_albums(), {
        credentials: "include",
      });
      if (!res.ok) throw new Error(`Admin albums API error: ${res.status}`);
      const data = (await res.json()) as AdminAlbum[];
      setAlbums(data);
      return data;
    } catch (e) {
      console.error("Failed to load admin albums:", e);
      setError(e instanceof Error ? e.message : "Unknown error");
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  const reloadMetadata = useCallback(async () => {
    setReloading(true);
    setError(null);
    setNotice(null);
    try {
      const prevIds = new Set(albums.map((a) => a.album_id));
      const res = await fetch(API_ENDPOINTS.admin_reload(), {
        method: "POST",
        credentials: "include",
      });
      if (!res.ok) {
        const msg = await res.text();
        throw new Error(`Reload failed: ${res.status} ${msg}`);
      }
      const summary = (await res.json()) as ReloadSummary;
      // Re-fetch the listing, then highlight albums absent from the previous view.
      const data = await fetchAlbums();
      if (data) {
        const added = data
          .filter((a) => !prevIds.has(a.album_id))
          .map((a) => a.album_id);
        setNewAlbumIds(new Set(added));
      }
      setNotice(
        `Reloaded: ${summary.added} added, ${summary.removed} removed, ` +
          `${summary.albums_after} albums total.`,
      );
    } catch (e) {
      console.error("Failed to reload metadata:", e);
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setReloading(false);
    }
  }, [albums, fetchAlbums]);

  useEffect(() => {
    const run = async () => {
      setLoading(true);
      setError(null);
      const ok = await checkSession();
      if (ok) {
        await fetchAlbums();
      } else {
        setAlbums([]);
        setLoading(false);
      }
    };
    void run();
  }, [checkSession, fetchAlbums]);

  const enabledCount = useMemo(
    () => albums.filter((a) => a.enabled).length,
    [albums],
  );

  const setAlbumEnabled = useCallback(
    async (albumId: string, enabled: boolean) => {
      setSavingIds((prev) => new Set(prev).add(albumId));

      // Optimistic update.
      setAlbums((prev) =>
        prev.map((a) => (a.album_id === albumId ? { ...a, enabled } : a)),
      );

      try {
        const res = await fetch(API_ENDPOINTS.admin_album_enabled(albumId), {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          credentials: "include",
          body: JSON.stringify({ enabled }),
        });
        if (!res.ok) throw new Error(`Admin set enabled error: ${res.status}`);
        const updated = (await res.json()) as AdminAlbum;
        setAlbums((prev) =>
          prev.map((a) => (a.album_id === albumId ? updated : a)),
        );
      } catch (e) {
        console.error("Failed to update album enabled:", e);
        setError(e instanceof Error ? e.message : "Unknown error");
        // Re-fetch to recover a consistent view.
        void fetchAlbums();
      } finally {
        setSavingIds((prev) => {
          const next = new Set(prev);
          next.delete(albumId);
          return next;
        });
      }
    },
    [fetchAlbums],
  );

  const submitLogin = useCallback(async () => {
    setLoginLoading(true);
    setError(null);
    try {
      const res = await fetch(API_ENDPOINTS.admin_login(), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({
          username: loginUsername,
          password: loginPassword,
        }),
      });
      if (!res.ok) {
        const msg = await res.text();
        throw new Error(`Login failed: ${res.status} ${msg}`);
      }
      setLoginPassword("");
      setLoggedIn(true);
      await fetchAlbums();
    } catch (e) {
      console.error("Admin login failed:", e);
      setError(e instanceof Error ? e.message : "Unknown error");
      setLoggedIn(false);
      setAlbums([]);
    } finally {
      setLoginLoading(false);
      setLoading(false);
    }
  }, [fetchAlbums, loginPassword, loginUsername]);

  const logout = useCallback(async () => {
    setError(null);
    try {
      await fetch(API_ENDPOINTS.admin_logout(), {
        method: "POST",
        credentials: "include",
      });
    } catch (e) {
      console.error("Admin logout failed:", e);
    } finally {
      setLoggedIn(false);
      setAlbums([]);
    }
  }, []);

  return (
    <div className="container">
      <div className="player-container">
        <h1 className="app-title">The Central Scrutinizer</h1>

        <div className="player-card scrutinizerCard">
          <div className="scrutinizerTopbar">
            <div className="scrutinizerTopbar__status">
              {loggedIn ? (
                <>
                  Enabled {enabledCount}/{albums.length}
                </>
              ) : (
                <>Login required</>
              )}
            </div>
            <div className="scrutinizerTopbar__actions">
              {loggedIn && (
                <button
                  type="button"
                  className="scrutinizer__button"
                  onClick={() => void reloadMetadata()}
                  disabled={reloading}
                  title="Re-read metadata from storage on the server"
                >
                  {reloading ? "Reloading..." : "Reload metadata"}
                </button>
              )}
              {loggedIn && (
                <button
                  type="button"
                  className="scrutinizer__button"
                  onClick={() => void fetchAlbums()}
                  disabled={reloading}
                >
                  Refresh
                </button>
              )}
              {loggedIn ? (
                <button
                  type="button"
                  className="scrutinizer__button"
                  onClick={() => void logout()}
                >
                  Logout
                </button>
              ) : null}
            </div>
          </div>

          {error && (
            <div className="scrutinizer__error">API Error: {error}</div>
          )}

          {notice && !error && (
            <div className="scrutinizer__notice">{notice}</div>
          )}

          {loading ? (
            <div className="scrutinizer__loading">Loading...</div>
          ) : !loggedIn ? (
            <form
              className="scrutinizerLogin"
              onSubmit={(e) => {
                e.preventDefault();
                void submitLogin();
              }}
            >
              <label className="scrutinizerLogin__row">
                <span className="scrutinizerLogin__label">Username</span>
                <input
                  className="scrutinizerLogin__input"
                  autoComplete="username"
                  value={loginUsername}
                  onChange={(e) => setLoginUsername(e.target.value)}
                />
              </label>
              <label className="scrutinizerLogin__row">
                <span className="scrutinizerLogin__label">Password</span>
                <input
                  className="scrutinizerLogin__input"
                  type="password"
                  autoComplete="current-password"
                  value={loginPassword}
                  onChange={(e) => setLoginPassword(e.target.value)}
                />
              </label>
              <button
                type="submit"
                className="scrutinizer__button scrutinizerLogin__submit"
                disabled={loginLoading}
              >
                {loginLoading ? "Logging in..." : "Login"}
              </button>
            </form>
          ) : (
            <ul className="scrutinizerAlbumList">
              {albums.map((album, index) => {
                const saving = savingIds.has(album.album_id);
                const isNew = newAlbumIds.has(album.album_id);
                const rowClass = album.enabled
                  ? "scrutinizerAlbumItem"
                  : "scrutinizerAlbumItem is-disabled";
                return (
                  <li
                    key={album.album_id}
                    className={rowClass}
                    title={album.album_id}
                  >
                    <div className="scrutinizerAlbumMain">
                      <div className="scrutinizerAlbumLeft">
                        <span className="scrutinizerAlbumNum">{index + 1}</span>
                      </div>

                      <div className="scrutinizerAlbumInfo">
                        <span className="scrutinizerAlbumTitle">
                          {album.scheduling_state === "playing" && (
                            <span className="scrutinizerAlbumBadge scrutinizerAlbumBadge--playing">
                              Now playing
                            </span>
                          )}
                          {album.scheduling_state === "queued" && (
                            <span className="scrutinizerAlbumBadge scrutinizerAlbumBadge--queued">
                              Queued
                            </span>
                          )}
                          {isNew && (
                            <span className="scrutinizerAlbumBadge scrutinizerAlbumBadge--new">
                              NEW
                            </span>
                          )}
                          <span className="scrutinizerAlbumName">
                            {album.album_name}
                          </span>
                        </span>
                        <span className="scrutinizerAlbumMeta">
                          {album.album_artist}
                        </span>
                      </div>

                      <div className="scrutinizerAlbumRight">
                        <span className="scrutinizerAlbumCount">
                          {album.track_count} tracks
                        </span>
                        <label
                          className="scrutinizerAlbumToggle"
                          aria-label="Enable album"
                        >
                          <input
                            className="scrutinizerAlbumCheckbox"
                            type="checkbox"
                            checked={album.enabled}
                            disabled={saving}
                            onChange={(e) =>
                              void setAlbumEnabled(
                                album.album_id,
                                e.target.checked,
                              )
                            }
                          />
                        </label>
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
};

export default Scrutinizer;
