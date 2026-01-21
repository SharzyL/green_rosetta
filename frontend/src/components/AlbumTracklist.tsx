import { type ReactElement, useEffect, useMemo, useRef, useState } from "react";
import "./AlbumTracklist.css";
import { ChevronDownIcon, ChevronUpIcon } from "./icons";

export interface AlbumTrack {
  track_id: string;
  track_title: string;
  track_artist?: string;
  track_number: number;
  disc_number: number;
  duration_seconds: number;
}

export interface AlbumResponse {
  album_id: string;
  album_name: string;
  album_artist: string;
  release_date?: string | null;
  original_release_date?: string | null;
  album_label?: string | null;
  album_media?: string | null;
  album_catalog_number?: string | null;
  musicbrainz_album_id?: string | null;
  disc_count: number;
  cover_url: string;
  tracks: AlbumTrack[];
}

interface AlbumTracklistProps {
  album: AlbumResponse | null;
  currentTrackId: string | null;
}

function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

const AlbumTracklist = ({
  album,
  currentTrackId,
}: AlbumTracklistProps): ReactElement => {
  const [expanded, setExpanded] = useState(false);
  const itemsWrapRef = useRef<HTMLDivElement | null>(null);

  const albumId = album?.album_id ?? null;
  useEffect(() => {
    // Reset on album switch.
    void albumId;
    setExpanded(false);
  }, [albumId]);

  const sortedTracks = useMemo(() => {
    if (!album) return [];
    return [...album.tracks].sort((a, b) => {
      const discDiff = a.disc_number - b.disc_number;
      if (discDiff !== 0) return discDiff;
      return a.track_number - b.track_number;
    });
  }, [album]);

  const trackSeqById = useMemo(() => {
    const m = new Map<string, number>();
    for (let i = 0; i < sortedTracks.length; i++) {
      m.set(sortedTracks[i].track_id, i + 1);
    }
    return m;
  }, [sortedTracks]);

  const totalTracks = sortedTracks.length;
  const currentSeq = currentTrackId
    ? (trackSeqById.get(currentTrackId) ?? null)
    : null;

  const musicbrainzUrl = album?.musicbrainz_album_id
    ? `https://musicbrainz.org/release/${encodeURIComponent(album.musicbrainz_album_id)}`
    : null;

  const label = album?.album_label?.trim() ? album.album_label : null;
  const date = album?.release_date?.trim() ? album.release_date : null;
  const originalDate = album?.original_release_date?.trim()
    ? album.original_release_date
    : null;

  const normalizedDate = date?.trim() ?? null;
  const normalizedOriginalDate = originalDate?.trim() ?? null;
  const showCombinedDate =
    normalizedDate &&
    normalizedOriginalDate &&
    normalizedDate === normalizedOriginalDate;
  const media = album?.album_media?.trim() ? album.album_media : null;
  const catalogNumberRaw = album?.album_catalog_number?.trim()
    ? album.album_catalog_number
    : null;
  const catalogNumber = catalogNumberRaw
    ? catalogNumberRaw.replace(/;\s*/g, ", ")
    : null;

  const items: ReactElement[] = [];
  let lastDisc: number | null = null;
  if (album) {
    for (const track of sortedTracks) {
      if (album.disc_count > 1 && lastDisc !== track.disc_number) {
        lastDisc = track.disc_number;
        items.push(
          <li
            key={`disc:${track.disc_number}`}
            className="album-tracklist-disc"
          >
            Disc {track.disc_number}
          </li>,
        );
      }

      const isCurrent = Boolean(
        currentTrackId && track.track_id === currentTrackId,
      );
      const classes = isCurrent
        ? "album-tracklist-item is-current"
        : "album-tracklist-item";
      items.push(
        <li key={track.track_id} className={classes}>
          <div className="album-tracklist-main">
            <div className="album-tracklist-left">
              <span className="album-tracklist-num">{track.track_number}</span>
            </div>
            <div className="album-tracklist-track">
              <span className="album-tracklist-track-title">
                {track.track_title}
              </span>
              <span className="album-tracklist-track-meta">
                {track.track_artist || album.album_artist}
              </span>
            </div>
            <div className="album-tracklist-right">
              <span className="album-tracklist-duration">
                {formatDuration(track.duration_seconds)}
              </span>
            </div>
          </div>
        </li>,
      );
    }
  }

  // Animate expand/fold by transitioning max-height. We measure the current content height
  // so the animation stays smooth regardless of track count.
  useEffect(() => {
    const el = itemsWrapRef.current;
    if (!el) return;

    if (expanded) {
      // Set an explicit height so CSS can transition from 0 -> measured px.
      const nextHeight = el.scrollHeight;
      el.style.maxHeight = `${nextHeight}px`;
      el.style.opacity = "1";
    } else {
      el.style.maxHeight = "0px";
      el.style.opacity = "0";
    }
  }, [expanded]);

  if (!album) {
    return (
      <div className="album-tracklist">
        <div className="album-tracklist-header">
          <h3 className="album-tracklist-title">Album Tracks</h3>
          <div className="album-tracklist-meta" aria-hidden="true">
            <div className="album-tracklist-meta-row" aria-hidden="true">
              <span className="album-tracklist-meta-key">ARTIST</span>
              <span className="album-tracklist-meta-value skeleton-line skeleton-line--metaValue" />
            </div>
            <div className="album-tracklist-meta-row" aria-hidden="true">
              <span className="album-tracklist-meta-key">DATE</span>
              <span className="album-tracklist-meta-value skeleton-line skeleton-line--metaValueShort" />
            </div>
            <div className="album-tracklist-meta-row" aria-hidden="true">
              <span className="album-tracklist-meta-key">LABEL</span>
              <span className="album-tracklist-meta-value skeleton-line skeleton-line--metaValue" />
            </div>
            <div className="album-tracklist-meta-row" aria-hidden="true">
              <span className="album-tracklist-meta-key">MEDIA</span>
              <span className="album-tracklist-meta-value skeleton-line skeleton-line--metaValue" />
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="album-tracklist">
      <div className="album-tracklist-header">
        <div className="album-tracklist-headerTop">
          <h3 className="album-tracklist-title">
            {musicbrainzUrl ? (
              <a
                className="album-tracklist-title-link"
                href={musicbrainzUrl}
                target="_blank"
                rel="noreferrer"
                title="Open in MusicBrainz"
              >
                {album.album_name}
              </a>
            ) : (
              album.album_name
            )}
            {currentSeq != null && totalTracks > 0 && (
              <span className="album-tracklist-title-suffix">
                {" "}
                ({currentSeq}/{totalTracks})
              </span>
            )}
          </h3>

          <button
            type="button"
            className="album-tracklist-toggle"
            onClick={() => setExpanded((v) => !v)}
            aria-expanded={expanded}
            aria-label={expanded ? "Collapse tracklist" : "Expand tracklist"}
            title={expanded ? "Collapse tracklist" : "Expand tracklist"}
          >
            {expanded ? <ChevronUpIcon /> : <ChevronDownIcon />}
          </button>
        </div>
        <div className="album-tracklist-meta">
          <div className="album-tracklist-meta-row">
            <span className="album-tracklist-meta-key">ARTIST</span>
            <span className="album-tracklist-meta-value">
              {album.album_artist}
            </span>
          </div>
          {showCombinedDate && normalizedDate && (
            <div className="album-tracklist-meta-row">
              <span className="album-tracklist-meta-key">DATE</span>
              <span className="album-tracklist-meta-value">
                {normalizedDate}
              </span>
            </div>
          )}
          {!showCombinedDate && normalizedOriginalDate && (
            <div className="album-tracklist-meta-row">
              <span className="album-tracklist-meta-key">ORIGINAL DATE</span>
              <span className="album-tracklist-meta-value">
                {normalizedOriginalDate}
              </span>
            </div>
          )}
          {!showCombinedDate && normalizedDate && (
            <div className="album-tracklist-meta-row">
              <span className="album-tracklist-meta-key">DATE</span>
              <span className="album-tracklist-meta-value">
                {normalizedDate}
              </span>
            </div>
          )}
          {label && (
            <div className="album-tracklist-meta-row">
              <span className="album-tracklist-meta-key">LABEL</span>
              <span className="album-tracklist-meta-value">{label}</span>
            </div>
          )}
          {media && (
            <div className="album-tracklist-meta-row">
              <span className="album-tracklist-meta-key">MEDIA</span>
              <span className="album-tracklist-meta-value">
                {catalogNumber ? `${media} (${catalogNumber})` : media}
              </span>
            </div>
          )}
          {!media && catalogNumber && (
            <div className="album-tracklist-meta-row">
              <span className="album-tracklist-meta-key">CATALOGNUMBER</span>
              <span className="album-tracklist-meta-value">
                {catalogNumber}
              </span>
            </div>
          )}
        </div>
      </div>
      <div
        ref={itemsWrapRef}
        className={[
          "album-tracklist-itemsWrap",
          expanded ? "is-expanded" : "is-collapsed",
        ].join(" ")}
        aria-hidden={!expanded}
      >
        <ol className="album-tracklist-items">{items}</ol>
      </div>
    </div>
  );
};

export default AlbumTracklist;
