import type { MediaPlayerClass } from "dashjs";
import { type ReactElement, type RefObject, useEffect, useState } from "react";
import "./DebugPanel.css";
import type { TrackInfoData } from "../App";

interface DebugPanelProps {
  videoRef: RefObject<HTMLVideoElement | null>;
  playerInstanceRef: RefObject<MediaPlayerClass | null>;
  nowPlaying: TrackInfoData | null;
  periodStartSeconds: number | null;
  autoplayBlocked: boolean;
  currentTrackId: string | null;
  onClose: () => void;
}

type Rows = Array<[string, string]>;
type Sections = Array<[string, Rows]>;

const fmt = (n: number | null | undefined, digits = 2): string =>
  typeof n === "number" && Number.isFinite(n) ? n.toFixed(digits) : "—";

const formatRanges = (ranges: TimeRanges | undefined): string => {
  if (!ranges || ranges.length === 0) return "none";
  const parts: string[] = [];
  for (let i = 0; i < ranges.length; i += 1) {
    parts.push(`${ranges.start(i).toFixed(1)}–${ranges.end(i).toFixed(1)}`);
  }
  return parts.join(", ");
};

// dash.js getters throw if called before the stream is initialized; swallow so a
// single not-yet-ready metric doesn't blank the whole panel.
const safe = (fn: () => unknown): void => {
  try {
    fn();
  } catch {
    /* dash.js not ready yet */
  }
};

function DebugPanel({
  videoRef,
  playerInstanceRef,
  nowPlaying,
  periodStartSeconds,
  autoplayBlocked,
  currentTrackId,
  onClose,
}: DebugPanelProps): ReactElement {
  const [sections, setSections] = useState<Sections>([]);

  useEffect(() => {
    const readSnapshot = (): Sections => {
      const video = videoRef.current;
      const player = playerInstanceRef.current;

      const media: Rows = [];
      if (video) {
        media.push(["paused", String(video.paused)]);
        media.push(["currentTime", fmt(video.currentTime)]);
        media.push(["muted", String(video.muted)]);
        media.push(["volume", fmt(video.volume)]);
        media.push(["playbackRate", fmt(video.playbackRate, 3)]);
        media.push(["readyState", String(video.readyState)]);
        media.push(["networkState", String(video.networkState)]);
        media.push(["buffered", formatRanges(video.buffered)]);
      } else {
        media.push(["video", "not attached"]);
      }

      const dash: Rows = [];
      if (player) {
        safe(() => dash.push(["version", player.getVersion()]));
        safe(() =>
          dash.push(["buffer(s)", fmt(player.getBufferLength("audio"))]),
        );
        safe(() => {
          const latency = player.getCurrentLiveLatency();
          const target = player.getTargetLiveDelay();
          dash.push(["liveLatency", fmt(latency)]);
          dash.push(["targetDelay", fmt(target)]);
          dash.push(["latencyOffset", fmt(latency - target)]);
        });
        safe(() => {
          const rep = player.getCurrentRepresentationForType("audio");
          if (rep) {
            dash.push(["repId", rep.id]);
            dash.push(["bitrate", `${Math.round(rep.bitrateInKbit)} kbps`]);
          }
          const streamId = player.getCurrentTrackFor("audio")?.streamInfo?.id;
          if (streamId) dash.push(["period", String(streamId)]);
        });
      } else {
        dash.push(["player", "not ready"]);
      }

      const inTrack =
        video && periodStartSeconds != null
          ? video.currentTime - periodStartSeconds
          : null;
      const app: Rows = [
        ["trackId", currentTrackId ?? "—"],
        ["albumId", nowPlaying?.album_id ?? "—"],
        [
          "track#",
          nowPlaying
            ? `${nowPlaying.disc_number}-${nowPlaying.track_number}`
            : "—",
        ],
        ["duration", fmt(nowPlaying?.duration_seconds, 1)],
        ["periodStart", fmt(periodStartSeconds)],
        ["elapsedInTrack", fmt(inTrack, 1)],
        ["autoplayBlocked", String(autoplayBlocked)],
      ];

      return [
        ["Media", media],
        ["DASH", dash],
        ["App", app],
      ];
    };

    setSections(readSnapshot());
    const interval = setInterval(() => setSections(readSnapshot()), 500);
    return () => clearInterval(interval);
  }, [
    videoRef,
    playerInstanceRef,
    nowPlaying,
    periodStartSeconds,
    autoplayBlocked,
    currentTrackId,
  ]);

  return (
    <div className="debug-panel">
      <div className="debug-panel__header">
        <span className="debug-panel__title">player debug</span>
        <button
          type="button"
          className="debug-panel__close"
          onClick={onClose}
          aria-label="Close debug panel"
        >
          ×
        </button>
      </div>
      <div className="debug-panel__body">
        {sections.map(([section, rows]) => (
          <div className="debug-panel__section" key={section}>
            <div className="debug-panel__sectionTitle">{section}</div>
            {rows.map(([key, value]) => (
              <div className="debug-panel__row" key={key}>
                <span className="debug-panel__key">{key}</span>
                <span className="debug-panel__value">{value}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

export default DebugPanel;
