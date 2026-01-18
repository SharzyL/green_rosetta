import {
	type ReactElement,
	type RefObject,
	useEffect,
	useMemo,
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
	onAutoplayRecovered: () => void;
}

function NowPlaying({
	nowPlaying,
	videoRef,
	periodStartSeconds,
	autoplayBlocked,
	onAutoplayRecovered,
}: NowPlayingProps): ReactElement {
	const isLoading = !nowPlaying;

	const [currentElapsed, setCurrentElapsed] = useState(0);
	const [coverStatus, setCoverStatus] = useState<
		"idle" | "loading" | "loaded" | "error"
	>("idle");

	useEffect(() => {
		if (!nowPlaying) {
			setCurrentElapsed(0);
			return;
		}
		setCurrentElapsed(0);
	}, [nowPlaying]);

	// Drive progress from the media element's playhead.
	useEffect(() => {
		if (!nowPlaying) return;
		if (periodStartSeconds == null) return;

		const interval = setInterval(() => {
			const video = videoRef.current;
			if (!video) return;

			const inTrackSeconds = video.currentTime - periodStartSeconds;
			if (!Number.isFinite(inTrackSeconds)) return;

			const clamped = Math.min(
				Math.max(inTrackSeconds, 0),
				nowPlaying.duration_seconds,
			);
			setCurrentElapsed(clamped);
		}, 200);

		return () => clearInterval(interval);
	}, [nowPlaying, periodStartSeconds, videoRef]);

	const coverUrl = nowPlaying?.cover_url ?? "";
	useEffect(() => {
		if (!coverUrl) {
			setCoverStatus("idle");
			return;
		}
		setCoverStatus("loading");
	}, [coverUrl]);

	const progress = useMemo(() => {
		if (!nowPlaying || nowPlaying.duration_seconds <= 0) return 0;
		return (currentElapsed / nowPlaying.duration_seconds) * 100;
	}, [currentElapsed, nowPlaying]);

	const isAutoplayBlocked = autoplayBlocked;
	const progressPct = isAutoplayBlocked || isLoading ? 0 : progress;
	const showCoverPlaceholder = isLoading || coverStatus !== "loaded";
	const showCoverSpinner = isLoading || coverStatus === "loading";
	const showTextSkeleton = isLoading;

	return (
		<div className="now-playing">
			<div className="cover-art">
				{showCoverPlaceholder && (
					<div className="cover-art__placeholder" aria-hidden="true">
						{showCoverSpinner && (
							<div className="cover-art__spinner" aria-hidden="true" />
						)}
					</div>
				)}
				{!!coverUrl && (
					<img
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
						onAutoplayRecovered={onAutoplayRecovered}
					/>
				</div>
			</div>
		</div>
	);
}

export default NowPlaying;
