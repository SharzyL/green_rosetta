import React, {
  type ReactElement,
  type RefObject,
  useEffect,
  useRef,
} from "react";
import "./Player.css";
import {
  MediaPlayer,
  type MediaPlayerClass,
  type PlaybackErrorEvent,
} from "dashjs";
import { loadMuted, loadVolume } from "../lib/volume";

interface PlayerProps {
  videoRef: RefObject<HTMLVideoElement | null>;
  manifestUrl: string;
  onAutoplayBlockedChange?: (blocked: boolean) => void;
  // Optional handle so callers can reach dash.js internals (debug polling, seeking
  // back to the live edge when recovering from blocked autoplay).
  playerInstanceRef?: RefObject<MediaPlayerClass | null>;
}

const Player = React.memo(
  ({
    videoRef,
    manifestUrl,
    onAutoplayBlockedChange,
    playerInstanceRef,
  }: PlayerProps): ReactElement => {
    // We attempt auto-play; if blocked by the browser, UI elsewhere can prompt for a gesture.
    const onAutoplayBlockedChangeRef =
      useRef<PlayerProps["onAutoplayBlockedChange"]>(null);
    const playerRef = useRef<MediaPlayerClass | null>(null);

    useEffect(() => {
      onAutoplayBlockedChangeRef.current = onAutoplayBlockedChange;
    }, [onAutoplayBlockedChange]);

    // Initialize DASH player once on mount
    useEffect(() => {
      const videoElement = videoRef.current;
      if (!videoElement || !manifestUrl) return;

      // Apply persisted audio settings early so autoplay uses the user's preferred volume.
      videoElement.volume = loadVolume(0.8);
      videoElement.muted = loadMuted();

      // Create and configure player
      const player = MediaPlayer().create();
      playerRef.current = player;
      if (playerInstanceRef) playerInstanceRef.current = player;

      // Configure DASH.js settings
      player.updateSettings({
        streaming: {
          abr: {
            autoSwitchBitrate: { audio: true },
          },
          buffer: {
            bufferToKeep: 30,
            bufferTimeAtTopQuality: 60,
            stallThreshold: 0.3,
          },
          gaps: {
            jumpGaps: false,
          },
          delay: {
            useSuggestedPresentationDelay: true,
          },
          liveCatchup: {
            enabled: true,
            playbackRate: {
              min: -0.02,
              max: 0.02,
            },
          },
          utcSynchronization: {
            enabled: true,
            useManifestDateHeaderTimeSource: false,
          },
          retryAttempts: {
            MPD: 3,
          },
          retryIntervals: {
            MPD: 500,
          },
        },
      });

      // Attach player to video element
      player.initialize(videoElement, manifestUrl, false);

      // Browsers refuse audible autoplay by default, but allow it muted. Falling back to muted
      // playback keeps the element genuinely playing, so dash.js goes on driving the playhead,
      // the period events and the manifest refresh, and nothing downstream needs a notion of a
      // stalled player -- the listener simply sees the volume control in its muted state.
      const attemptAutoplay = async () => {
        try {
          await videoElement.play();
          onAutoplayBlockedChangeRef.current?.(false);
          return;
        } catch (err) {
          console.warn("Audible autoplay refused; retrying muted:", err);
        }

        videoElement.muted = true;

        try {
          await videoElement.play();
          onAutoplayBlockedChangeRef.current?.(false);
        } catch (err) {
          // Even muted playback was refused, so nothing moves until a gesture arrives.
          console.warn("Muted autoplay refused:", err);
          onAutoplayBlockedChangeRef.current?.(true);
        }
      };

      const handleStreamInitialized = () => {
        void attemptAutoplay();
      };

      const handlePlaybackError = (e: PlaybackErrorEvent) => {
        console.error("DASH playback error:", e);
      };

      const handlePeriodSwitch = () => {
        const currentLatency =
          player.getCurrentLiveLatency() - player.getTargetLiveDelay();

        if (currentLatency < -1.0) {
          const waitTimeSeconds = 1.0;
          const waitTimeMs = waitTimeSeconds * 1000;

          player.pause();

          setTimeout(() => {
            player.play();
          }, waitTimeMs);
        }
      };

      const handleVideoPlay = () => {
        onAutoplayBlockedChangeRef.current?.(false);
      };

      // Register DASH.js event listeners
      const events = MediaPlayer.events;

      player.on(events.STREAM_INITIALIZED, handleStreamInitialized);
      player.on(events.PLAYBACK_ERROR, handlePlaybackError);
      player.on(events.PERIOD_SWITCH_COMPLETED, handlePeriodSwitch);

      // Register video element event listeners
      videoElement.addEventListener("play", handleVideoPlay);

      // Cleanup on unmount
      return () => {
        player.off(events.STREAM_INITIALIZED, handleStreamInitialized);
        player.off(events.PLAYBACK_ERROR, handlePlaybackError);
        player.off(events.PERIOD_SWITCH_COMPLETED, handlePeriodSwitch);

        videoElement.removeEventListener("play", handleVideoPlay);

        player.destroy();
        playerRef.current = null;
        if (playerInstanceRef) playerInstanceRef.current = null;
      };
    }, [manifestUrl, videoRef, playerInstanceRef]);

    return (
      <div className="player-wrapper">
        {/* biome-ignore lint/a11y/useMediaCaption: We stream audio-only content and don't provide captions. */}
        <video ref={videoRef} className="player-media" />
      </div>
    );
  },
);

Player.displayName = "Player";

export default Player;
