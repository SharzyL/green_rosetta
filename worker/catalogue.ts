import { parse as parseYaml } from "yaml";
import { z } from "zod";

// Per-album YAML written by the generator under `metadata/`. Only the fields the worker reads are
// checked; anything else the generator writes passes through untouched.

// The YAML is read with every scalar kept as a string (see `parseAlbumYaml`), so numbers are
// converted here, field by field, as serde did on the Rust side.
const num = z.coerce.number();
const optionalString = z.string().nullish();

const SegmentTimelineEntrySchema = z.object({
  t: num.nullish(),
  d: num,
  r: num.nullish(),
});

const RepresentationSchema = z
  .object({
    name: z.string(),
    adaptation_set_id: optionalString,
    // Older generator builds misspelled the key.
    adaption_set_id: optionalString,
    codec: z.string(),
    mpd_codecs: z.string(),
    bandwidth: num,
    sample_rate: num,
    channels: num,
    init_segment_path: z.string(),
    segment_path_template: z.string(),
    segment_count: num,
    segment_timescale: num.nullish(),
    segment_timeline: z.array(SegmentTimelineEntrySchema).nullish(),
    encoded_length_seconds: num.nullish(),
  })
  .transform(({ adaption_set_id, ...rep }) => ({
    ...rep,
    adaptation_set_id: rep.adaptation_set_id ?? adaption_set_id,
  }));

const TrackSchema = z.object({
  id: z.string(),
  title: z.string(),
  artist: optionalString,
  disc_number: num,
  track_number: num,
  length_seconds: num,
  representations: z.array(RepresentationSchema).default([]),
  // Legacy single-representation fields.
  encoded_length_seconds: num.nullish(),
  init_segment_path: optionalString,
  segment_path_template: optionalString,
  segment_count: num.nullish(),
  segment_duration: num.nullish(),
  segment_timescale: num.nullish(),
  segment_timeline: z.array(SegmentTimelineEntrySchema).nullish(),
});

const AlbumSchema = z.object({
  id: z.string(),
  name: z.string(),
  artist: z.string(),
  release_date: optionalString,
  original_release_date: optionalString,
  media: optionalString,
  label: optionalString,
  catalog_number: optionalString,
  musicbrainz_album_id: optionalString,
  disc_count: num,
  cover_path: z.string(),
  tracks: z.array(TrackSchema),
});

export type SegmentTimelineEntry = z.infer<typeof SegmentTimelineEntrySchema>;
export type Representation = z.output<typeof RepresentationSchema>;
export type Track = z.infer<typeof TrackSchema>;
export type Album = z.infer<typeof AlbumSchema>;

/** Parse one album YAML file. `source` names it in errors. */
export function parseAlbumYaml(content: string, source: string): Album {
  let raw: unknown;
  try {
    // Only `null` is resolved by YAML itself. Under the default schema an unquoted id such as
    // `85e7385738` would turn into Infinity and `0123456789` would lose its leading zero.
    raw = parseYaml(content, { schema: "failsafe", customTags: ["null"] });
  } catch (e) {
    throw new Error(`Parse YAML in '${source}': ${(e as Error).message}`);
  }
  const parsed = AlbumSchema.safeParse(raw);
  if (!parsed.success) {
    throw new Error(
      `Invalid album metadata in '${source}': ${z.prettifyError(parsed.error)}`,
    );
  }
  return parsed.data;
}

function timelineSeconds(
  timescale: number,
  timeline: SegmentTimelineEntry[],
): number {
  let ticks = 0;
  for (const entry of timeline) {
    const repeats = entry.r ?? 0;
    if (repeats < 0) continue;
    ticks += entry.d * (repeats + 1);
  }
  return ticks / timescale;
}

/** The track's representations, with legacy metadata exposed as a single "default" one. */
export function effectiveRepresentations(track: Track): Representation[] {
  if (track.representations.length > 0) return track.representations;

  const { init_segment_path, segment_path_template, segment_count } = track;
  if (
    init_segment_path == null ||
    segment_path_template == null ||
    segment_count == null
  )
    return [];

  return [
    {
      name: "default",
      adaptation_set_id: null,
      codec: "aac",
      mpd_codecs: "mp4a.40.2",
      bandwidth: 128_000,
      sample_rate: 48_000,
      channels: 2,
      init_segment_path,
      segment_path_template,
      segment_count,
      segment_timescale: track.segment_timescale,
      segment_timeline: track.segment_timeline,
      encoded_length_seconds: track.encoded_length_seconds,
    },
  ];
}

/**
 * Duration derived from the encoded output timeline when available, so scheduling matches the
 * MPD Period duration. Falls back to encoded_length_seconds, then segment_count * segment_duration.
 */
export function encodedDurationSeconds(track: Track): number {
  const segmentDuration = track.segment_duration ?? 6.0;

  if (track.representations.length > 0) {
    // With several AdaptationSets, pick a stable timing reference so Period boundaries stay
    // consistent for most clients: prefer AAC-in-MP4, then any rep with a timeline.
    const reps = track.representations;
    const rep =
      reps.find((r) => r.mpd_codecs.startsWith("mp4a")) ??
      reps.find(
        (r) => r.segment_timescale != null && r.segment_timeline != null,
      ) ??
      reps[0];

    if (rep.segment_timescale && rep.segment_timeline) {
      return timelineSeconds(rep.segment_timescale, rep.segment_timeline);
    }
    if (rep.encoded_length_seconds != null) return rep.encoded_length_seconds;
    return rep.segment_count * segmentDuration;
  }

  if (track.segment_timescale && track.segment_timeline) {
    return timelineSeconds(track.segment_timescale, track.segment_timeline);
  }
  if (track.encoded_length_seconds != null) return track.encoded_length_seconds;
  return segmentDuration * (track.segment_count ?? 0);
}

/** All albums, in metadata key order. Lookups are linear; the catalogue is small. */
export class Catalogue {
  constructor(readonly albums: Album[]) {}

  getAlbum(id: string): Album | undefined {
    return this.albums.find((a) => a.id === id);
  }

  getTrack(trackId: string): { album: Album; track: Track } | undefined {
    for (const album of this.albums) {
      const track = album.tracks.find((t) => t.id === trackId);
      if (track) return { album, track };
    }
    return undefined;
  }

  getAlbumTrack(albumId: string, trackId: string): Track | undefined {
    return this.getAlbum(albumId)?.tracks.find((t) => t.id === trackId);
  }

  get totalTracks(): number {
    return this.albums.reduce((n, a) => n + a.tracks.length, 0);
  }
}

const METADATA_PREFIX = "metadata/";

/** Read every `metadata/*.yaml` album from the bucket. Any malformed file fails the whole load. */
export async function loadCatalogue(bucket: R2Bucket): Promise<Catalogue> {
  const keys: string[] = [];
  let cursor: string | undefined;
  do {
    const listing = await bucket.list({ prefix: METADATA_PREFIX, cursor });
    for (const obj of listing.objects) {
      const ext = obj.key.split(".").pop()?.toLowerCase();
      if (ext === "yaml" || ext === "yml") keys.push(obj.key);
    }
    cursor = listing.truncated ? listing.cursor : undefined;
  } while (cursor);

  const albums = await Promise.all(
    keys.map(async (key) => {
      const obj = await bucket.get(key);
      if (!obj)
        throw new Error(`Metadata object '${key}' vanished while loading`);
      return parseAlbumYaml(await obj.text(), key);
    }),
  );

  if (albums.length === 0) {
    throw new Error(`No album YAML files found under '${METADATA_PREFIX}'`);
  }
  return new Catalogue(albums);
}
