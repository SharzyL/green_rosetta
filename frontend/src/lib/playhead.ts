/**
 * Readers are cached per element so the anchor survives re-renders and track changes.
 */
const readers = new WeakMap<HTMLVideoElement, () => number>();

/**
 * Reads where the station is now, in presentation-timeline seconds.
 *
 * While playing this is the media element's playhead. While the element is stopped -- a browser
 * that refuses even muted autoplay, so nothing moves until a gesture -- the playhead is frozen,
 * but the station keeps broadcasting, so the reading carries on off the wall clock.
 *
 * The reader is shared per element and must not be re-created by callers: its anchor accumulates
 * the wall clock since the playhead last moved, so a fresh reader would re-anchor to the frozen
 * `currentTime` and report the station as still being wherever it was when playback stalled.
 */
export function playheadFor(video: HTMLVideoElement): () => number {
  const cached = readers.get(video);
  if (cached) return cached;

  let anchor = { mediaTime: video.currentTime, atMs: performance.now() };

  const read = () => {
    const now = performance.now();

    // Re-anchor whenever the playhead moves, so this follows playback exactly and is already in
    // the right place the moment it freezes again.
    if (anchor.mediaTime !== video.currentTime) {
      anchor = { mediaTime: video.currentTime, atMs: now };
    }

    return anchor.mediaTime + (now - anchor.atMs) / 1000;
  };

  readers.set(video, read);
  return read;
}
