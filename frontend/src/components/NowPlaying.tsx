import {
  type ReactElement,
  type RefObject,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import "./NowPlaying.css";
import VolumeControl from "./VolumeControl";

interface NowPlayingData {
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

interface NowPlayingProps {
  nowPlaying: NowPlayingData | null;
  videoRef: RefObject<HTMLVideoElement>;
  periodStartSeconds: number | null;
  autoplayBlocked: boolean;
  onRequestPlay: () => void;
  onActivateDebug?: () => void;
}

function NowPlaying({
  nowPlaying,
  videoRef,
  periodStartSeconds,
  autoplayBlocked,
  onRequestPlay,
  onActivateDebug,
}: NowPlayingProps): ReactElement {
  const isLoading = !nowPlaying;

  const [currentElapsed, setCurrentElapsed] = useState(0);
  const [coverStatus, setCoverStatus] = useState<
    "idle" | "loading" | "loaded" | "error"
  >("idle");
  const imgRef = useRef<HTMLImageElement>(null);
  const debugClicksRef = useRef<number[]>([]);
  // Anchor used to extrapolate the playhead while it is frozen; see the progress effect.
  const frozenAnchorRef = useRef<{ mediaTime: number; atMs: number } | null>(
    null,
  );

  // Reveal the hidden debug panel after five clicks on the cover within 2s
  // (the classic "tap the version number" gesture).
  const handleCoverClick = () => {
    if (!onActivateDebug) return;
    const now = performance.now();
    const recent = [...debugClicksRef.current, now].filter(
      (t) => now - t < 2000,
    );
    debugClicksRef.current = recent;
    if (recent.length >= 5) {
      debugClicksRef.current = [];
      onActivateDebug();
    }
  };

  useEffect(() => {
    if (!nowPlaying) {
      setCurrentElapsed(0);
      return;
    }
    setCurrentElapsed(0);
  }, [nowPlaying]);

  useEffect(() => {
    if (!autoplayBlocked) frozenAnchorRef.current = null;
  }, [autoplayBlocked]);

  // Drive progress from the media element's playhead.
  useEffect(() => {
    if (!nowPlaying) return;
    if (periodStartSeconds == null) return;

    const interval = setInterval(() => {
      const video = videoRef.current;
      if (!video) return;

      let mediaTime = video.currentTime;

      if (autoplayBlocked) {
        // The playhead is frozen while we wait for a gesture, but the station keeps
        // broadcasting. Anchor to wherever dash.js parked the playhead and advance it
        // off the wall clock so the bar still reflects what is on air. Re-anchoring
        // whenever `currentTime` moves absorbs dash.js's initial seek to the live edge.
        const now = performance.now();
        const anchor = frozenAnchorRef.current;
        const nextAnchor =
          !anchor || anchor.mediaTime !== mediaTime
            ? { mediaTime, atMs: now }
            : anchor;

        frozenAnchorRef.current = nextAnchor;
        mediaTime = nextAnchor.mediaTime + (now - nextAnchor.atMs) / 1000;
      }

      const inTrackSeconds = mediaTime - periodStartSeconds;
      if (!Number.isFinite(inTrackSeconds)) return;

      const clamped = Math.min(
        Math.max(inTrackSeconds, 0),
        nowPlaying.duration_seconds,
      );
      setCurrentElapsed(clamped);
    }, 200);

    return () => clearInterval(interval);
  }, [nowPlaying, periodStartSeconds, videoRef, autoplayBlocked]);

  const coverUrl = nowPlaying?.cover_url ?? "";
  useEffect(() => {
    if (!coverUrl) {
      setCoverStatus("idle");
      return;
    }
    setCoverStatus("loading");

    // Cached images may already be complete by the time this effect runs, so the
    // <img> onLoad event never fires. Reconcile immediately to avoid getting stuck
    // in the "loading" state.
    const img = imgRef.current;
    if (img?.complete) {
      setCoverStatus(img.naturalWidth > 0 ? "loaded" : "error");
    }
  }, [coverUrl]);

  const progress = useMemo(() => {
    if (!nowPlaying || nowPlaying.duration_seconds <= 0) return 0;
    return (currentElapsed / nowPlaying.duration_seconds) * 100;
  }, [currentElapsed, nowPlaying]);

  const progressPct = isLoading ? 0 : progress;
  const showCoverPlaceholder = isLoading || coverStatus !== "loaded";
  const showCoverSpinner = isLoading || coverStatus === "loading";
  const showTextSkeleton = isLoading;

  return (
    <div className="now-playing">
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: hidden debug gesture, intentionally not keyboard-exposed */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: hidden debug gesture, intentionally not keyboard-exposed */}
      <div className="cover-art" onClick={handleCoverClick}>
        {showCoverPlaceholder && (
          <div className="cover-art__placeholder" aria-hidden="true">
            {showCoverSpinner && (
              <div className="cover-art__spinner" aria-hidden="true" />
            )}
          </div>
        )}
        {!!coverUrl && (
          <img
            ref={imgRef}
            className={
              showCoverPlaceholder
                ? "cover-art__img is-loading"
                : "cover-art__img"
            }
            src={coverUrl}
            alt={nowPlaying?.album_name ?? "Cover"}
            onLoad={() => setCoverStatus("loaded")}
            onError={() => setCoverStatus("error")}
          />
        )}
      </div>

      <div className="progress-bar">
        <div className="progress-fill" style={{ width: `${progressPct}%` }} />
      </div>

      <div className="track-info">
        <div className="track-info__text">
          {showTextSkeleton ? (
            <div className="now-playing__skeletonText" aria-hidden="true">
              <div className="skeleton-line skeleton-line--title" />
              <div className="skeleton-line skeleton-line--artist" />
            </div>
          ) : (
            <>
              <h2 className="track-title">{nowPlaying?.track_title}</h2>
              <p className="track-artist">
                {nowPlaying?.track_artist || "Unknown Artist"}
              </p>
            </>
          )}
        </div>
        <div className="track-info__right">
          <VolumeControl
            videoRef={videoRef}
            autoplayBlocked={autoplayBlocked}
            onRequestPlay={onRequestPlay}
          />
        </div>
      </div>
    </div>
  );
}

export default NowPlaying;
