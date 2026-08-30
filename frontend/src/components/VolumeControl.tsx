import type React from "react";
import {
  type ReactElement,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import "./VolumeControl.css";
import {
  clamp01,
  loadMuted,
  loadVolume,
  saveMuted,
  saveVolume,
} from "../lib/volume";
import { PlayIcon, VolumeOffIcon, VolumeOnIcon } from "./icons";

interface VolumeControlProps {
  videoRef: RefObject<HTMLVideoElement>;
  autoplayBlocked: boolean;
  onRequestPlay: () => void;
}

const VolumeControl = ({
  videoRef,
  autoplayBlocked,
  onRequestPlay,
}: VolumeControlProps): ReactElement => {
  const [volume, setVolume] = useState<number>(() => loadVolume(0.8));
  const [muted, setMuted] = useState<boolean>(() => loadMuted());
  const lastNonZeroVolumeRef = useRef<number>(volume > 0 ? volume : 0.8);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const sliderTrackRef = useRef<HTMLDivElement | null>(null);
  const popupRef = useRef<HTMLDivElement | null>(null);

  const volumePct = useMemo(() => Math.round(volume * 100), [volume]);
  // When muted, present the slider at 0 while keeping the real volume value persisted.
  const sliderValuePct = muted ? 0 : volumePct;

  // Silence has two representations: the `muted` flag, and a volume dragged to zero.
  const isSilent = muted || volume === 0;

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = volume;
  }, [videoRef, volume]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = muted;
  }, [videoRef, muted]);

  const handleVolumeChange = (next: number) => {
    const clamped = clamp01(next);
    setVolume(clamped);
    saveVolume(clamped);

    if (clamped > 0) lastNonZeroVolumeRef.current = clamped;

    if (muted && clamped > 0) {
      setMuted(false);
      saveMuted(false);
    }
  };

  // Leave either silent state: clear `muted`, and restore the remembered level if the
  // volume itself was dragged to zero (which overwrote the level).
  const ensureAudible = () => {
    const video = videoRef.current;
    const nextVolume = volume > 0 ? volume : lastNonZeroVolumeRef.current;

    if (nextVolume !== volume) {
      setVolume(nextVolume);
      saveVolume(nextVolume);
    }
    if (muted) {
      setMuted(false);
      saveMuted(false);
    }

    // Callers may start playback within the same click, before the volume/muted
    // effects run, so apply to the element directly instead of awaiting the commit.
    if (video) {
      video.volume = nextVolume;
      video.muted = false;
    }
  };

  const handleToggleMute = () => {
    // Toggle audibility rather than the `muted` flag: at volume 0 the button already
    // reads "Unmute", so flipping `muted` would silence an already silent player and
    // cost a second click to undo.
    if (isSilent) {
      ensureAudible();
      return;
    }

    setMuted(true);
    saveMuted(true);
  };

  const handleBlockedPlayClick = () => {
    // A saved mute would otherwise start playback silently and need a second click.
    ensureAudible();
    onRequestPlay();
  };

  const showSilentStyle = autoplayBlocked || isSilent;
  const muteButtonClassName = [
    "volume__mute",
    showSilentStyle ? "volume__mute--silent" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const sliderClassName = [
    "volume__slider",
    muted ? "volume__slider--muted" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const rootClassName = ["volume", autoplayBlocked ? "volume--blocked" : ""]
    .filter(Boolean)
    .join(" ");

  const setFromClientX = (clientX: number) => {
    const track = sliderTrackRef.current;
    if (!track) return;
    const rect = track.getBoundingClientRect();
    const x = clientX - rect.left;
    const pct = rect.width > 0 ? x / rect.width : 0;
    handleVolumeChange(pct);
  };

  const handleSliderPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    // We implement our own slider to avoid UA-specific range styling differences.
    e.preventDefault();
    (e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId);
    setFromClientX(e.clientX);
  };

  const handleSliderPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    setFromClientX(e.clientX);
  };

  const handleSliderKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = 0.05; // 5% per keypress
    if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      handleVolumeChange(volume - step);
    } else if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      handleVolumeChange(volume + step);
    } else if (e.key === "Home") {
      e.preventDefault();
      handleVolumeChange(0);
    } else if (e.key === "End") {
      e.preventDefault();
      handleVolumeChange(1);
    }
  };

  const updatePopupPosition = useCallback(() => {
    const popup = popupRef.current;
    if (!popup) return;

    // Reset before measuring so we compute overflow against the centered position.
    popup.style.setProperty("--volume-popup-shift", "0px");

    const rect = popup.getBoundingClientRect();
    const gutter = 8;
    const overflowLeft = Math.max(0, gutter - rect.left);
    const overflowRight = Math.max(
      0,
      rect.right - (window.innerWidth - gutter),
    );
    const shiftPx = overflowLeft - overflowRight;

    popup.style.setProperty("--volume-popup-shift", `${shiftPx}px`);
  }, []);

  useEffect(() => {
    // Keep the popup within the viewport on resize.
    const handleResize = () => updatePopupPosition();
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [updatePopupPosition]);

  useEffect(() => {
    // On touch devices, tapping outside often doesn't move focus (many elements aren't focusable),
    // which can leave the popup stuck open. Force-blur when clicking outside the control.
    const handlePointerDown = (e: PointerEvent) => {
      const root = rootRef.current;
      if (!root) return;

      const target = e.target as Node | null;
      if (target && root.contains(target)) return;

      const active = document.activeElement as HTMLElement | null;
      if (active && root.contains(active)) active.blur();
    };

    document.addEventListener("pointerdown", handlePointerDown, true);
    return () =>
      document.removeEventListener("pointerdown", handlePointerDown, true);
  }, []);

  return (
    <div
      ref={rootRef}
      className={rootClassName}
      onPointerEnter={() => updatePopupPosition()}
      onFocusCapture={() => updatePopupPosition()}
    >
      <button
        type="button"
        className={muteButtonClassName}
        onClick={autoplayBlocked ? handleBlockedPlayClick : handleToggleMute}
        aria-label={
          autoplayBlocked ? "Click to play" : isSilent ? "Unmute" : "Mute"
        }
        title={autoplayBlocked ? "Click to play" : isSilent ? "Unmute" : "Mute"}
      >
        {autoplayBlocked ? (
          <PlayIcon />
        ) : isSilent ? (
          <VolumeOffIcon />
        ) : (
          <VolumeOnIcon />
        )}
      </button>
      {!autoplayBlocked && !muted && (
        <div ref={popupRef} className="volume__popup">
          <div
            ref={sliderTrackRef}
            className={sliderClassName}
            role="slider"
            tabIndex={0}
            aria-label="Volume"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={sliderValuePct}
            onPointerDown={handleSliderPointerDown}
            onPointerMove={handleSliderPointerMove}
            onKeyDown={handleSliderKeyDown}
          >
            <div className="volume__sliderTrack" />
            <div
              className="volume__sliderFill"
              style={{ width: `${sliderValuePct}%` }}
            />
            <div
              className="volume__sliderThumb"
              style={{ left: `${sliderValuePct}%` }}
            />
          </div>
        </div>
      )}
    </div>
  );
};

export default VolumeControl;
