import type { Representation, Track } from "./catalogue";
import { effectiveRepresentations } from "./catalogue";

export interface DashConfig {
  /** Prefix of every init/media URL, e.g. `https://media.example.com/media`. */
  mediaBaseUrl: string;
  timeShiftBufferDepth: number;
  suggestedPresentationDelay: number;
}

export interface MpdPeriod {
  track: Track;
  startSeconds: number;
  durationSeconds: number;
}

function xmlEscapeAttr(raw: string): string {
  return raw
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

const encoder = new TextEncoder();

/**
 * Percent-encode a path for embedding in the MPD, keeping `/` separators and the `$` / `%` of
 * DASH `$Number%03d$` templates. Everything outside a conservative ASCII allowlist is encoded as
 * UTF-8 bytes.
 */
export function urlEncodePathish(raw: string): string {
  let out = "";
  for (const ch of raw) {
    if (/^[A-Za-z0-9\-_.~/$%]$/.test(ch)) {
      out += ch;
    } else {
      for (const b of encoder.encode(ch))
        out += `%${b.toString(16).toUpperCase().padStart(2, "0")}`;
    }
  }
  return out;
}

/** Turn the generator's `chunk_%03d$.m4s` style template into a DASH `$Number%03d$` one. */
export function toDashNumberTemplate(template: string): string {
  if (template.includes("$Number")) return template;
  if (template.includes("%03d$"))
    return template.replace("%03d$", "$Number%03d$");
  if (template.includes("%03d"))
    return template.replace("%03d", "$Number%03d$");
  return template;
}

function segmentTimeline(
  rep: Representation,
  fallbackSegmentDuration: number,
): { timescale: number; xml: string } {
  if (rep.segment_timescale != null && rep.segment_timeline != null) {
    const xml = rep.segment_timeline
      .map((entry) => {
        let attrs = "";
        if (entry.t != null) attrs += ` t="${entry.t}"`;
        attrs += ` d="${entry.d}"`;
        if (entry.r != null) attrs += ` r="${entry.r}"`;
        return `            <S${attrs} />`;
      })
      .join("\n");
    return { timescale: rep.segment_timescale, xml };
  }

  // Fallback: a constant-duration timeline at the representation timescale if known.
  const timescale = rep.segment_timescale ?? 1000;
  const ticks = Math.trunc(fallbackSegmentDuration * timescale);
  const xml =
    rep.segment_count > 0
      ? `            <S t="0" d="${ticks}" r="${rep.segment_count - 1}" />`
      : "";
  return { timescale, xml };
}

function representationXml(
  rep: Representation,
  mediaBaseUrl: string,
  fallbackSegmentDuration: number,
): string {
  const { timescale, xml } = segmentTimeline(rep, fallbackSegmentDuration);
  const base = mediaBaseUrl.replace(/\/+$/, "");
  const init = `${base}/${urlEncodePathish(rep.init_segment_path.replace(/^\/+/, ""))}`;
  const media = `${base}/${urlEncodePathish(toDashNumberTemplate(rep.segment_path_template).replace(/^\/+/, ""))}`;

  return `      <Representation id="${xmlEscapeAttr(rep.name)}" codecs="${xmlEscapeAttr(rep.mpd_codecs)}" audioSamplingRate="${rep.sample_rate}" bandwidth="${rep.bandwidth}">
        <AudioChannelConfiguration schemeIdUri="urn:mpeg:dash:23003:3:audio_channel_configuration:2011" value="${rep.channels}"/>
        <SegmentTemplate timescale="${timescale}"
                         initialization="${xmlEscapeAttr(init)}"
                         media="${xmlEscapeAttr(media)}"
                         startNumber="1">
          <SegmentTimeline>
${xml}
          </SegmentTimeline>
        </SegmentTemplate>
      </Representation>`;
}

function periodXml(period: MpdPeriod, mediaBaseUrl: string): string {
  const { track } = period;
  const reps = effectiveRepresentations(track);
  const fallbackSegmentDuration = track.segment_duration ?? 6.0;

  // Group representations into AdaptationSets by the generator's adaptation_set_id, so unrelated
  // variant groups sharing a codec string stay apart. Legacy metadata groups by codec.
  const setKey = (rep: Representation) =>
    rep.adaptation_set_id ?? rep.mpd_codecs;
  const setKeys = [...new Set(reps.map(setKey))].sort();

  const adaptationSets = setKeys
    .map((key) => {
      const repsXml = reps
        .filter((rep) => setKey(rep) === key)
        .map((rep) =>
          representationXml(rep, mediaBaseUrl, fallbackSegmentDuration),
        )
        .join("\n");
      return `    <AdaptationSet mimeType="audio/mp4" segmentAlignment="true" startWithSAP="1">
${repsXml}
    </AdaptationSet>`;
    })
    .join("\n");

  // Period starts are kept at millisecond precision.
  const startSeconds = Math.trunc(period.startSeconds * 1000) / 1000;
  return `  <Period id="${xmlEscapeAttr(`period_${track.id}`)}" start="PT${startSeconds.toFixed(3)}S" duration="PT${period.durationSeconds.toFixed(3)}S">
${adaptationSets}
  </Period>`;
}

/** A dynamic multi-period MPD: one Period per scheduled track in the window. */
export function generateMpd(
  config: DashConfig,
  availabilityStartMs: number,
  nowMs: number,
  periods: MpdPeriod[],
): string {
  const tsbd = Math.max(1, config.timeShiftBufferDepth);
  const spd = Math.max(0, config.suggestedPresentationDelay);
  const periodsXml = periods
    .map((p) => periodXml(p, config.mediaBaseUrl))
    .join("\n");
  const now = new Date(nowMs).toISOString();

  // The clock rides along in the MPD itself (`direct`), so a player can sync without another
  // round trip before its first segment; `/api/utc` stays as the fallback for players that
  // skip the direct scheme.
  return `<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     type="dynamic"
     availabilityStartTime="${new Date(availabilityStartMs).toISOString()}"
     publishTime="${now}"
     minimumUpdatePeriod="PT30S"
     timeShiftBufferDepth="PT${tsbd.toFixed(0)}S"
     suggestedPresentationDelay="PT${spd.toFixed(0)}S"
     minBufferTime="PT2S">
  <UTCTiming schemeIdUri="urn:mpeg:dash:utc:direct:2014" value="${now}" />
  <UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014" value="/api/utc" />
${periodsXml}
</MPD>`;
}
