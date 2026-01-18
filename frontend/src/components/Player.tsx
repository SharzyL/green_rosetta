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
	type PeriodSwitchEvent,
	type PlaybackErrorEvent,
} from "dashjs";
import { loadMuted, loadVolume } from "../lib/volume";

interface PlayerProps {
	videoRef: RefObject<HTMLVideoElement | null>;
	manifestUrl: string;
	onAutoplayBlockedChange?: (blocked: boolean) => void;
	onTrackIdChange?: (trackId: string) => void;
	onPeriodStartSecondsChange?: (startSeconds: number) => void;
}

const Player = React.memo(
	({
		videoRef,
		manifestUrl,
		onAutoplayBlockedChange,
		onTrackIdChange,
		onPeriodStartSecondsChange,
	}: PlayerProps): ReactElement => {
		// We attempt auto-play; if blocked by the browser, UI elsewhere can prompt for a gesture.
		const onAutoplayBlockedChangeRef =
			useRef<PlayerProps["onAutoplayBlockedChange"]>(null);
		const onTrackIdChangeRef = useRef<PlayerProps["onTrackIdChange"]>(null);
		const onPeriodStartSecondsChangeRef =
			useRef<PlayerProps["onPeriodStartSecondsChange"]>(null);
		const playerRef = useRef<MediaPlayerClass | null>(null);

		useEffect(() => {
			onAutoplayBlockedChangeRef.current = onAutoplayBlockedChange;
		}, [onAutoplayBlockedChange]);

		useEffect(() => {
			onTrackIdChangeRef.current = onTrackIdChange;
		}, [onTrackIdChange]);

		useEffect(() => {
			onPeriodStartSecondsChangeRef.current = onPeriodStartSecondsChange;
		}, [onPeriodStartSecondsChange]);

		// Initialize DASH player once on mount
		useEffect(() => {
			const videoElement = videoRef.current;
			if (!videoElement || !manifestUrl) return;

			// Apply persisted audio settings early so autoplay uses the user's preferred volume.
			videoElement.volume = loadVolume(0.8);
			videoElement.muted = loadMuted();

			console.log("Initializing DASH player");

			// Create and configure player
			const player = MediaPlayer().create();
			playerRef.current = player;

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

			// Define event handlers inside useEffect to avoid dependency issues
			const attemptAutoplay = async () => {
				try {
					await videoElement.play();
					onAutoplayBlockedChangeRef.current?.(false);
				} catch (err) {
					// Autoplay blocked; require a user gesture.
					console.warn("Autoplay blocked:", err);
					onAutoplayBlockedChangeRef.current?.(true);
				}
			};

			const handleStreamInitialized = () => {
				attemptAutoplay();

				// Best-effort initial period -> track mapping.
				const streamInfo = player.getActiveStream?.()?.getStreamInfo?.();
				const streamInfoId = streamInfo?.id as string | undefined;
				if (streamInfoId?.startsWith("period_")) {
					const trackId = streamInfoId.slice("period_".length);
					onTrackIdChangeRef.current?.(trackId);
				}

				if (typeof streamInfo?.start === "number") {
					onPeriodStartSecondsChangeRef.current?.(streamInfo.start);
				}
			};

			const handlePlaybackError = (e: PlaybackErrorEvent) => {
				console.error("DASH playback error:", e);
			};

			const handlePeriodSwitch = (e: PeriodSwitchEvent) => {
				const currentLatency =
					player.getCurrentLiveLatency() - player.getTargetLiveDelay();

				console.log(
					`Period switch: latency=${currentLatency}`,
					e.toStreamInfo?.id,
				);

				const periodId = e.toStreamInfo?.id;
				if (typeof periodId === "string" && periodId.startsWith("period_")) {
					const trackId = periodId.slice("period_".length);
					onTrackIdChangeRef.current?.(trackId);
				}

				if (typeof e.toStreamInfo?.start === "number") {
					onPeriodStartSecondsChangeRef.current?.(e.toStreamInfo.start);
				}

				if (currentLatency < -1.0) {
					const waitTimeSeconds = 1.0;
					const waitTimeMs = waitTimeSeconds * 1000;
					console.log(`Period switch: will pause for ${waitTimeMs}ms (${e})`);

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

			const diagnosticInterval = setInterval(() => {
				console.log("debug[live latency]", {
					clientTime: new Date().toISOString(),
					offset: player.getCurrentLiveLatency() - player.getTargetLiveDelay(),
				});
			}, 10000);

			// Cleanup on unmount
			return () => {
				player.off(events.STREAM_INITIALIZED, handleStreamInitialized);
				player.off(events.PLAYBACK_ERROR, handlePlaybackError);
				player.off(events.PERIOD_SWITCH_COMPLETED, handlePeriodSwitch);

				videoElement.removeEventListener("play", handleVideoPlay);

				player.destroy();
				clearInterval(diagnosticInterval);
				playerRef.current = null;
			};
		}, [manifestUrl, videoRef]);

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
