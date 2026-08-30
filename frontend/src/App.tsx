import type { MediaPlayerClass } from "dashjs";
import {
  type ReactElement,
  type RefObject,
  useCallback,
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
  // A rejected play() often arrives after the listener has clicked again; a second
  // concurrent play() would only abort the first one.
  const resumeInFlightRef = useRef(false);

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

  const handleRequestPlay = useCallback(async () => {
    const video = videoRef.current;
    if (!video) return;
    if (resumeInFlightRef.current) return;

    resumeInFlightRef.current = true;

    // dash.js parked the playhead at the live edge it computed on load and left it
    // there while we waited for a gesture, so it is now stale by however long the
    // block lasted. Rejoin the live edge before resuming, otherwise playback starts
    // behind where the station actually is.
    playerInstanceRef.current?.seekToOriginalLive();

    try {
      await video.play();

      // Engines that predate the play() promise resolve immediately without starting;
      // a still-paused element means the gesture did not take.
      if (video.paused) {
        throw new DOMException("Playback did not start", "NotAllowedError");
      }

      setAutoplayBlocked(false);
    } catch (err) {
      // `NotAllowedError` here means the gesture did not lift the block; `AbortError`
      // means the start was interrupted. Either way keep the play button up so the
      // listener can retry; `AbortError` can arrive after `play` already fired and
      // cleared the flag, so re-assert it rather than assuming it held.
      const name = err instanceof DOMException ? err.name : "unknown";
      console.warn(`Play failed after user gesture (${name}):`, err);
      setAutoplayBlocked(true);
    } finally {
      resumeInFlightRef.current = false;
    }
  }, []);

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
            onRequestPlay={handleRequestPlay}
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
