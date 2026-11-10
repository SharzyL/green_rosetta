import { describe, expect, it } from "vitest";

import {
  effectiveRepresentations,
  encodedDurationSeconds,
  parseAlbumYaml,
} from "../catalogue";
import { album, albumYaml } from "./fixtures";

describe("parseAlbumYaml", () => {
  it("reads a multi-disc album in generator order", () => {
    const a = album("x", [2, 3]);
    expect(a.disc_count).toBe(2);
    expect(a.tracks.map((t) => [t.disc_number, t.track_number])).toEqual([
      [1, 1],
      [1, 2],
      [2, 1],
      [2, 2],
      [2, 3],
    ]);
    // Same track number on different discs must not share segments.
    expect(a.tracks[0].representations[0].init_segment_path).toContain(
      "track_1_1/",
    );
    expect(a.tracks[2].representations[0].init_segment_path).toContain(
      "track_2_1/",
    );
  });

  it("keeps quoted and unquoted strings as strings", () => {
    const a = album("x", [1]);
    expect(a.release_date).toBe("1962");
    expect(a.media).toBe('12" Vinyl');
    expect(a.musicbrainz_album_id).toBeNull();
  });

  it("keeps unquoted ids that look like numbers as written", () => {
    // The generator writes hex ids unquoted; YAML would read these as Infinity and 123456789.
    const yaml = albumYaml("x", [1])
      .replace(/^id: x$/m, "id: 0123456789")
      .replace("id: x_d1t1", "id: 85e7385738");
    const a = parseAlbumYaml(yaml, "x.yaml");
    expect(a.id).toBe("0123456789");
    expect(a.tracks[0].id).toBe("85e7385738");
    expect(a.tracks[0].length_seconds).toBe(11.5);
  });

  it("accepts the legacy adaption_set_id spelling", () => {
    const yaml = albumYaml("x", [1]).replaceAll(
      "adaptation_set_id:",
      "adaption_set_id:",
    );
    const rep = parseAlbumYaml(yaml, "x.yaml").tracks[0].representations[0];
    expect(rep.adaptation_set_id).toBe("as0_opus");
  });

  it("names the file and field when metadata is malformed", () => {
    const yaml = albumYaml("x", [1]).replace(/^cover_path: .*$/m, "");
    expect(() => parseAlbumYaml(yaml, "metadata/x.yaml")).toThrow(
      /metadata\/x\.yaml[\s\S]*cover_path/,
    );
    expect(() => parseAlbumYaml("id: [", "metadata/y.yaml")).toThrow(
      /metadata\/y\.yaml/,
    );
  });
});

describe("encodedDurationSeconds", () => {
  it("sums the AAC representation's timeline", () => {
    const track = album("x", [1], 3).tracks[0];
    track.representations[0].segment_timeline = [{ t: 0, d: 1, r: 0 }]; // opus: ignored
    expect(encodedDurationSeconds(track)).toBe(18);
  });

  it("falls back through legacy fields", () => {
    const base = { ...album("x", [1]).tracks[0], representations: [] };
    expect(
      encodedDurationSeconds({
        ...base,
        segment_timescale: 1000,
        segment_timeline: [{ d: 2500, r: 3 }],
      }),
    ).toBe(10);
    expect(
      encodedDurationSeconds({ ...base, encoded_length_seconds: 7.5 }),
    ).toBe(7.5);
    expect(
      encodedDurationSeconds({
        ...base,
        segment_count: 4,
        segment_duration: 2,
      }),
    ).toBe(8);
  });

  it("exposes legacy metadata as one default representation", () => {
    const base = { ...album("x", [1]).tracks[0], representations: [] };
    expect(effectiveRepresentations(base)).toEqual([]);
    const reps = effectiveRepresentations({
      ...base,
      init_segment_path: "a/init.mp4",
      segment_path_template: "a/chunk_%03d.m4s",
      segment_count: 3,
    });
    expect(reps).toHaveLength(1);
    expect(reps[0]).toMatchObject({
      name: "default",
      mpd_codecs: "mp4a.40.2",
      segment_count: 3,
    });
  });
});
