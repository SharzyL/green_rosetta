import { stringify } from "yaml";

import type { Album } from "../catalogue";
import { Catalogue, parseAlbumYaml } from "../catalogue";

export const TIMESCALE = 48000;

/** A track of `segments` 6-second segments, in the shape the generator writes. */
export function trackYaml(
  albumDir: string,
  id: string,
  disc: number,
  number: number,
  segments = 2,
) {
  const rep = (
    name: string,
    setId: string,
    codec: string,
    mpdCodecs: string,
  ) => {
    const dir = `albums/${albumDir}/track_${disc}_${number}/${name}`;
    return {
      name,
      adaptation_set_id: setId,
      codec,
      mpd_codecs: mpdCodecs,
      bandwidth: 128000,
      sample_rate: 48000,
      channels: 2,
      init_segment_path: `${dir}/init.mp4`,
      segment_path_template: `${dir}/chunk_%03d$.m4s`,
      segment_count: segments,
      segment_timescale: TIMESCALE,
      segment_timeline: [{ t: 0, d: 6 * TIMESCALE, r: segments - 1 }],
      encoded_length_seconds: 6 * segments,
    };
  };
  return {
    id,
    title: `Track ${disc}-${number}`,
    artist: "Artist",
    disc_number: disc,
    track_number: number,
    length_seconds: 6 * segments - 0.5,
    representations: [
      rep("as0_opus_128k", "as0_opus", "opus", "opus"),
      rep("as1_aac_128k", "as1_aac", "aac", "mp4a.40.2"),
    ],
    source_file_path: `/music/${id}.flac`,
    created_at: "2026-05-20T11:47:36.280495657+00:00",
  };
}

/** Album YAML with `discs[i]` tracks on disc i+1. Track ids are `{id}_d{disc}t{n}`. */
export function albumYaml(id: string, discs: number[], segments = 2): string {
  const dir = `${id} - Artist - Album ${id}`;
  const tracks = discs.flatMap((count, d) =>
    Array.from({ length: count }, (_, i) =>
      trackYaml(dir, `${id}_d${d + 1}t${i + 1}`, d + 1, i + 1, segments),
    ),
  );
  return stringify({
    id,
    name: `Album ${id}`,
    artist: "Artist",
    release_date: "1962",
    original_release_date: "1962",
    media: '12" Vinyl',
    label: "impulse!",
    catalog_number: "A 16",
    musicbrainz_album_id: null,
    disc_count: discs.length,
    cover_path: `albums/${dir}/cover.jpg`,
    created_at: "2026-05-20T11:47:36.280495657+00:00",
    updated_at: "2026-05-20T11:47:36.280495657+00:00",
    images: [
      {
        id: `${id}_img`,
        image_path: `albums/${dir}/cover.jpg`,
        image_type: "cover",
        is_primary: true,
      },
    ],
    tracks,
  });
}

export function album(id: string, discs: number[], segments = 2): Album {
  return parseAlbumYaml(albumYaml(id, discs, segments), `${id}.yaml`);
}

/** Single-disc albums of `tracks` 12-second tracks each. */
export function catalogue(spec: Record<string, number>): Catalogue {
  return new Catalogue(
    Object.entries(spec).map(([id, tracks]) => album(id, [tracks])),
  );
}
