import type { Album, Catalogue, Track } from "./catalogue";
import { encodedDurationSeconds } from "./catalogue";

/** A track placed on the presentation timeline, in seconds since `epochMs`. */
export interface ScheduledTrack {
  albumId: string;
  trackId: string;
  startSeconds: number;
  /** Encoded duration, so it matches the MPD Period. */
  durationSeconds: number;
}

/** Everything the station must keep across restarts. Plain data, so it can be stored as is. */
export interface ScheduleState {
  /** MPD `availabilityStartTime`. Fixed for the life of the station: moving it breaks every client. */
  epochMs: number;
  /** The manifest window: tracks covering roughly `[t_now - tsbd, t_now + min_future]`. */
  queue: ScheduledTrack[];
  /**
   * The last track ever appended, retained after it leaves `queue`.
   *
   * `queue` is trimmed to the window, so once a quiet spell ages every entry out it can no longer
   * answer "where has the catalogue reached". This cursor can.
   */
  lastScheduled: ScheduledTrack | null;
}

export interface WindowConfig {
  timeShiftBufferDepth: number;
  minFutureManifestDuration: number;
}

/** A point-in-time view of which albums are on air or queued ahead. */
export interface SchedulingSnapshot {
  nowPlayingAlbumId: string | null;
  /** Albums with tracks starting in the future part of the window (may include the playing one). */
  queuedAlbumIds: Set<string>;
}

export const endSeconds = (st: ScheduledTrack): number =>
  st.startSeconds + st.durationSeconds;

const enabledAlbums = (
  catalogue: Catalogue,
  disabled: ReadonlySet<string>,
): Album[] => catalogue.albums.filter((a) => !disabled.has(a.id));

/**
 * The station's playlist: every enabled album in catalogue order, start to end, wrapping around.
 *
 * Nothing runs on a timer. Each request first calls `maintain`, which slides the window up to
 * now; after a quiet spell it plays the playlist forward through the gap, so a listener who comes
 * back finds the station where it would have been had it played to an empty room.
 */
export class Scheduler {
  constructor(
    readonly state: ScheduleState,
    public catalogue: Catalogue,
    public disabled: ReadonlySet<string>,
    private readonly random: () => number = Math.random,
  ) {}

  /** Start a fresh station whose first track is already `delaySeconds` old at `nowMs`. */
  static create(
    catalogue: Catalogue,
    disabled: ReadonlySet<string>,
    nowMs: number,
    delaySeconds: number,
    random: () => number = Math.random,
  ): Scheduler {
    // Shift availabilityStartTime back by the presentation delay so clients starting at
    // (live_edge - delay) have media immediately.
    const epochMs = nowMs - Math.max(0, delaySeconds) * 1000;
    const scheduler = new Scheduler(
      { epochMs, queue: [], lastScheduled: null },
      catalogue,
      disabled,
      random,
    );
    const { album, track } = scheduler.randomStart();
    scheduler.push(album.id, track, 0);
    return scheduler;
  }

  secondsAt(nowMs: number): number {
    return (nowMs - this.state.epochMs) / 1000;
  }

  /**
   * Slide the window to `nowMs`: drop tracks fully behind it, drop tracks the catalogue no longer
   * has, and extend the future. Returns whether the state changed and needs persisting.
   */
  maintain(nowMs: number, window: WindowConfig): boolean {
    const { queue } = this.state;
    const before = JSON.stringify(this.state);

    const tNow = this.secondsAt(nowMs);
    const windowStart = Math.max(0, tNow - window.timeShiftBufferDepth);
    const windowEnd = tNow + Math.max(0, window.minFutureManifestDuration);

    // Without this, a deleted in-window track keeps "covering" its slot, so nothing backfills it
    // and the manifest renders nothing for that span.
    const resolvable = queue.filter((st) =>
      this.catalogue.getAlbumTrack(st.albumId, st.trackId),
    );
    if (resolvable.length < queue.length) {
      console.warn(
        `Pruned ${queue.length - resolvable.length} unresolvable scheduled track(s) after metadata change`,
      );
      queue.splice(0, queue.length, ...resolvable);
    }

    while (queue.length > 0 && endSeconds(queue[0]) < windowStart)
      queue.shift();

    if (queue.length === 0) this.resume(tNow, windowStart);

    // Each track starts where the previous ends, which also fills any span freed by pruning.
    while (endSeconds(queue[queue.length - 1]) < windowEnd) {
      const last = queue[queue.length - 1];
      const { album, track } = this.nextInPlaylist(last.albumId, last.trackId);
      this.push(album.id, track, endSeconds(last));
    }

    return JSON.stringify(this.state) !== before;
  }

  /**
   * Rejoin the playlist after the window emptied: either a quiet spell aged it all out, or pruning
   * removed every entry.
   */
  private resume(tNow: number, windowStart: number): void {
    const cursor = this.state.lastScheduled;
    if (!cursor) {
      console.warn(
        "schedule queue ran dry with no playlist cursor; starting from a random album",
      );
      const { album, track } = this.randomStart();
      this.push(album.id, track, tNow);
      return;
    }

    // A cursor ahead of now means pruning emptied the queue under a future track; start the
    // replacement at now rather than leave silence until that track would have ended.
    let start = Math.min(endSeconds(cursor), tNow);
    let { album, track } = this.nextInPlaylist(cursor.albumId, cursor.trackId);
    let duration = this.durationOf(track);

    // Play through the quiet spell: skip every track that would already have finished.
    while (start + duration < windowStart) {
      start += duration;
      ({ album, track } = this.nextInPlaylist(album.id, track.id));
      duration = this.durationOf(track);
    }

    this.push(album.id, track, start);
  }

  private durationOf(track: Track): number {
    const duration = encodedDurationSeconds(track);
    // A zero-length track would never move the window forward.
    if (!(duration > 0))
      throw new Error(`Track ${track.id} has no encoded duration`);
    return duration;
  }

  /** Append to the window and advance the playlist cursor together. */
  private push(albumId: string, track: Track, startSeconds: number): void {
    const scheduled: ScheduledTrack = {
      albumId,
      trackId: track.id,
      startSeconds,
      durationSeconds: this.durationOf(track),
    };
    this.state.queue.push(scheduled);
    this.state.lastScheduled = scheduled;
  }

  /** Pick a random enabled album and its first track. */
  private randomStart(): { album: Album; track: Track } {
    const enabled = enabledAlbums(this.catalogue, this.disabled);
    if (enabled.length === 0) throw new Error("No enabled albums");
    const album = enabled[Math.floor(this.random() * enabled.length)];
    const track = album.tracks[0];
    if (!track) throw new Error("Enabled album has no tracks");
    return { album, track };
  }

  /** The track that follows `(albumId, trackId)` in the playlist. */
  nextInPlaylist(
    albumId: string,
    trackId: string,
  ): { album: Album; track: Track } {
    // The album may have been removed by a reload; treat that the same as disabled.
    const current = this.catalogue.getAlbum(albumId);
    const currentEnabled = current !== undefined && !this.disabled.has(albumId);

    if (currentEnabled) {
      const pos = current.tracks.findIndex((t) => t.id === trackId);
      if (pos !== -1 && pos + 1 < current.tracks.length) {
        return { album: current, track: current.tracks[pos + 1] };
      }
    }

    const enabled = enabledAlbums(this.catalogue, this.disabled);
    if (enabled.length === 0) throw new Error("No enabled albums");

    // With the current album gone or disabled there is no list position to continue from. Pick at
    // random rather than always the first album, which would make every deletion resume there.
    if (!currentEnabled) return this.randomStart();

    const pos = enabled.findIndex((a) => a.id === albumId);
    for (const album of enabled.slice(pos + 1)) {
      if (album.tracks.length > 0) return { album, track: album.tracks[0] };
    }

    // Wrap to the first enabled album.
    const first = enabled[0];
    if (first.tracks.length === 0)
      throw new Error("Enabled album has no tracks");
    return { album: first, track: first.tracks[0] };
  }

  /** What is scheduled at a point on the timeline, if the window covers it. */
  scheduledAt(seconds: number): ScheduledTrack | undefined {
    return this.state.queue.find(
      (st) => st.startSeconds <= seconds && seconds < endSeconds(st),
    );
  }

  /** The tracks overlapping the manifest window at `nowMs`, with their metadata. */
  periods(
    nowMs: number,
    window: WindowConfig,
  ): { track: Track; scheduled: ScheduledTrack }[] {
    const tNow = this.secondsAt(nowMs);
    const windowStart = Math.max(0, tNow - window.timeShiftBufferDepth);
    const windowEnd = tNow + Math.max(0, window.minFutureManifestDuration);

    const out: { track: Track; scheduled: ScheduledTrack }[] = [];
    for (const scheduled of this.state.queue) {
      if (
        endSeconds(scheduled) <= windowStart ||
        scheduled.startSeconds >= windowEnd
      )
        continue;
      const track = this.catalogue.getAlbumTrack(
        scheduled.albumId,
        scheduled.trackId,
      );
      // `maintain` prunes vanished tracks, so this only skips if it could not run.
      if (track) out.push({ track, scheduled });
    }
    return out;
  }

  snapshot(nowMs: number): SchedulingSnapshot {
    const tNow = this.secondsAt(nowMs);
    const snap: SchedulingSnapshot = {
      nowPlayingAlbumId: null,
      queuedAlbumIds: new Set(),
    };
    for (const st of this.state.queue) {
      if (st.startSeconds <= tNow && tNow < endSeconds(st)) {
        snap.nowPlayingAlbumId = st.albumId;
      } else if (st.startSeconds > tNow) {
        snap.queuedAlbumIds.add(st.albumId);
      }
    }
    return snap;
  }
}
