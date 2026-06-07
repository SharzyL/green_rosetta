import type { MediaPlayerClass } from "dashjs";
import {
  type ReactElement,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import "./App.css";
import AlbumTracklist, {
  type AlbumResponse,
} from "./components/AlbumTracklist";
import DebugPanel from "./components/DebugPanel";
import NowPlaying from "./components/NowPlaying";
import Player from "./components/Player";
import { API_ENDPOINTS } from "./config";

export interface TrackInfoData {
  album_id: string;
  album_name: string;
  album_artist: string;
  track_id: string;
  track_title: string;
  track_artist?: string;
  track_number: number;
  disc_number: number;
  duration_seconds: number;
  cover_url: string;
}

function App(): ReactElement {
  const [trackInfo, setTrackInfo] = useState<TrackInfoData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const playerInstanceRef = useRef<MediaPlayerClass | null>(null);
  const [currentAlbum, setCurrentAlbum] = useState<AlbumResponse | null>(null);
  const [autoplayBlocked, setAutoplayBlocked] = useState(false);
  const [currentTrackId, setCurrentTrackId] = useState<string | null>(null);
  const [currentPeriodStartSeconds, setCurrentPeriodStartSeconds] = useState<
    number | null
  >(null);
  const [debugMode, setDebugMode] = useState(false);

  useEffect(() => {
    const trackId = currentTrackId;
    if (!trackId) {
      setTrackInfo(null);
      return;
    }

    // Clear immediately so UI doesn't keep showing the previous track/album
    // while the next track metadata request is in flight.
    setTrackInfo(null);
    setError(null);

    const controller = new AbortController();
    const isAbortError = (err: unknown) =>
      err instanceof DOMException && err.name === "AbortError";

    const fetchTrackInfo = async () => {
      try {
        const response = await fetch(API_ENDPOINTS.track(trackId), {
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`Track API error: ${response.status}`);
        }
        const data = (await response.json()) as TrackInfoData;
        setTrackInfo(data);
        setError(null);
      } catch (err) {
        if (isAbortError(err)) return;
        console.error("Failed to fetch track info:", err);
        setError(err instanceof Error ? err.message : "Unknown error");
        setTrackInfo(null);
      }
    };

    void fetchTrackInfo();

    return () => controller.abort();
  }, [currentTrackId]);

  useEffect(() => {
    const albumId = trackInfo?.album_id;
    if (!albumId) {
      setCurrentAlbum(null);
      return;
    }

    // Clear immediately so UI shows skeleton while the new album loads.
    setCurrentAlbum(null);

    const controller = new AbortController();
    const isAbortError = (err: unknown) =>
      err instanceof DOMException && err.name === "AbortError";

    const fetchAlbum = async () => {
      try {
        const response = await fetch(API_ENDPOINTS.album(albumId), {
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`Album API error: ${response.status}`);
        }
        const data = (await response.json()) as AlbumResponse;
        setCurrentAlbum(data);
      } catch (err) {
        if (isAbortError(err)) return;
        console.error("Failed to fetch album:", err);
        setCurrentAlbum(null);
      }
    };

    void fetchAlbum();

    return () => controller.abort();
  }, [trackInfo?.album_id]);

  return (
    <div className="container">
      <div className="player-container">
        <h1 className="app-title">a little green rosetta</h1>
        <div className="player-card">
          {error && <div className="error-banner">⚠️ API Error: {error}</div>}

          <Player
            videoRef={videoRef}
            manifestUrl={API_ENDPOINTS.manifest()}
            onAutoplayBlockedChange={setAutoplayBlocked}
            onTrackIdChange={setCurrentTrackId}
            onPeriodStartSecondsChange={setCurrentPeriodStartSeconds}
            playerInstanceRef={playerInstanceRef}
          />

          <NowPlaying
            nowPlaying={trackInfo}
            videoRef={videoRef as RefObject<HTMLVideoElement>}
            periodStartSeconds={currentPeriodStartSeconds}
            autoplayBlocked={autoplayBlocked}
            onAutoplayRecovered={() => setAutoplayBlocked(false)}
            onActivateDebug={() => setDebugMode((v) => !v)}
          />

          <AlbumTracklist
            album={currentAlbum}
            currentTrackId={currentTrackId}
          />
        </div>
      </div>

      {debugMode && (
        <DebugPanel
          videoRef={videoRef}
          playerInstanceRef={playerInstanceRef}
          nowPlaying={trackInfo}
          periodStartSeconds={currentPeriodStartSeconds}
          autoplayBlocked={autoplayBlocked}
          currentTrackId={currentTrackId}
          onClose={() => setDebugMode(false)}
        />
      )}
    </div>
  );
}

export default App;
