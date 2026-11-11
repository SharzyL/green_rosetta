import { describe, expect, it } from "vitest";

import { generateMpd, toDashNumberTemplate, urlEncodePathish } from "../dash";
import { album } from "./fixtures";

describe("toDashNumberTemplate", () => {
  it("turns generator templates into $Number$ templates", () => {
    expect(toDashNumberTemplate("a/chunk_%03d$.m4s")).toBe(
      "a/chunk_$Number%03d$.m4s",
    );
    expect(toDashNumberTemplate("a/chunk_%03d.m4s")).toBe(
      "a/chunk_$Number%03d$.m4s",
    );
    expect(toDashNumberTemplate("a/chunk_$Number%05d$.m4s")).toBe(
      "a/chunk_$Number%05d$.m4s",
    );
    expect(toDashNumberTemplate("a/init.mp4")).toBe("a/init.mp4");
  });
});

describe("urlEncodePathish", () => {
  it("keeps separators and template characters, encodes the rest as UTF-8", () => {
    expect(
      urlEncodePathish("albums/x - It’s_Time/chunk_$Number%03d$.m4s"),
    ).toBe("albums/x%20-%20It%E2%80%99s_Time/chunk_$Number%03d$.m4s");
  });
});

describe("generateMpd", () => {
  const config = {
    mediaBaseUrl: "https://media.example.com/media/",
    timeShiftBufferDepth: 60,
    suggestedPresentationDelay: 30,
  };
  const track = album("x", [1]).tracks[0];
  const mpd = generateMpd(
    config,
    Date.UTC(2026, 0, 1),
    Date.UTC(2026, 0, 1, 1),
    [{ track, startSeconds: 12.3456, durationSeconds: 12 }],
  );

  it("describes a dynamic stream anchored at the station epoch", () => {
    expect(mpd).toContain('type="dynamic"');
    expect(mpd).toContain('availabilityStartTime="2026-01-01T00:00:00.000Z"');
    expect(mpd).toContain('publishTime="2026-01-01T01:00:00.000Z"');
    expect(mpd).toContain('timeShiftBufferDepth="PT60S"');
    expect(mpd).toContain('suggestedPresentationDelay="PT30S"');
    // The direct clock comes first so players sync without another round trip.
    expect(mpd).toMatch(
      /<UTCTiming schemeIdUri="urn:mpeg:dash:utc:direct:2014" value="2026-01-01T01:00:00.000Z" \/>\s*<UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014" value="\/api\/utc" \/>/,
    );
  });

  it("emits one period per track with one AdaptationSet per variant group", () => {
    expect(mpd).toContain(
      `<Period id="period_${track.id}" start="PT12.345S" duration="PT12.000S">`,
    );
    expect(mpd.match(/<AdaptationSet /g)).toHaveLength(2);
    expect(mpd).toContain(
      '<Representation id="as1_aac_128k" codecs="mp4a.40.2" audioSamplingRate="48000" bandwidth="128000">',
    );
    expect(mpd).toContain('<S t="0" d="288000" r="1" />');
  });

  it("points segment URLs at the public media origin", () => {
    expect(mpd).toContain(
      'initialization="https://media.example.com/media/albums/x%20-%20Artist%20-%20Album%20x/track_1_1/as0_opus_128k/init.mp4"',
    );
    expect(mpd).toContain(
      'media="https://media.example.com/media/albums/x%20-%20Artist%20-%20Album%20x/track_1_1/as0_opus_128k/chunk_$Number%03d$.m4s"',
    );
  });

  it("falls back to a constant timeline for legacy metadata", () => {
    const legacy = {
      ...track,
      representations: [],
      init_segment_path: "legacy/init.mp4",
      segment_path_template: "legacy/chunk_%03d.m4s",
      segment_count: 5,
      segment_duration: 4,
    };
    const out = generateMpd(config, 0, 0, [
      { track: legacy, startSeconds: 0, durationSeconds: 20 },
    ]);
    expect(out).toContain('<SegmentTemplate timescale="1000"');
    expect(out).toContain('<S t="0" d="4000" r="4" />');
  });
});
