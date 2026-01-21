use crate::config;
use crate::db::{Track, TrackRepresentation};
use chrono::{Duration, Utc};

pub struct DashGenerator {
    media_base_url: String,
    time_shift_buffer_depth_seconds: f64,
    suggested_presentation_delay_seconds: f64,
}

fn xml_escape_attr(raw: &str) -> String {
    // Minimal XML attribute escaping.
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn url_encode_pathish(raw: &str) -> String {
    // Percent-encode path characters so URLs embedded in the MPD are safe.
    //
    // We intentionally keep:
    // - '/' path separators
    // - '$' and '%' because DASH SegmentTemplate uses `$Number%03d$` formatting.
    //
    // Everything else outside a conservative ASCII allowlist is encoded as UTF-8 bytes.
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let keep =
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | '/' | '$' | '%');
        if keep {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for b in ch.encode_utf8(&mut buf).as_bytes() {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

impl DashGenerator {
    pub fn new(media_base_url: String, streaming: &config::StreamingConfig) -> Self {
        DashGenerator {
            media_base_url,
            time_shift_buffer_depth_seconds: streaming.time_shift_buffer_depth,
            suggested_presentation_delay_seconds: streaming.suggested_presentation_delay,
        }
    }

    /// Generate DASH MPD manifest for multi-period seamless playback
    pub fn generate_mpd(
        &self,
        availability_start_time: chrono::DateTime<Utc>,
        periods: Vec<(Track, f64, f64)>,
    ) -> anyhow::Result<String> {
        let now = Utc::now();
        let now_iso = now.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string();
        let availability_start_time = availability_start_time
            .format("%Y-%m-%dT%H:%M:%S.%3fZ")
            .to_string();

        let mut period_infos = Vec::new();
        for (track, start_seconds, duration_seconds) in periods {
            period_infos.push(PeriodInfo {
                id: format!("period_{}", track.id),
                start_time: Duration::milliseconds((start_seconds * 1000.0) as i64),
                duration: duration_seconds,
                track,
            });
        }

        let periods_xml = generate_periods(&period_infos, &self.media_base_url);

        let time_shift_buffer_depth_seconds = self.time_shift_buffer_depth_seconds.max(1.0);
        let suggested_presentation_delay_seconds =
            self.suggested_presentation_delay_seconds.max(0.0);

        let mpd = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     type="dynamic"
     availabilityStartTime="{}"
     publishTime="{}"
     minimumUpdatePeriod="PT30S"
     timeShiftBufferDepth="PT{:.0}S"
     suggestedPresentationDelay="PT{:.0}S"
     minBufferTime="PT2S">
  <UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014" value="/api/utc" />
{}
</MPD>"#,
            availability_start_time,
            now_iso,
            time_shift_buffer_depth_seconds,
            suggested_presentation_delay_seconds,
            periods_xml
        );

        Ok(mpd)
    }
}

#[derive(Clone)]
struct PeriodInfo {
    id: String,
    start_time: Duration,
    duration: f64,
    track: Track,
}

fn generate_periods(periods: &[PeriodInfo], media_base_url: &str) -> String {
    let mut result = String::new();

    for period in periods {
        let start_seconds = period.start_time.num_milliseconds() as f64 / 1000.0;

        let reps = period.track.effective_representations();
        let fallback_seg_dur = period.track.segment_duration.unwrap_or(6.0);

        // Group representations into AdaptationSets.
        //
        // - Prefer metadata-provided adaptation_set_id (from generator config), so we don't merge
        //   unrelated variant groups that happen to share the same codec string.
        // - Fall back to mpd_codecs for legacy metadata (single set per codec).
        let mut set_keys: Vec<String> = reps
            .iter()
            .map(|r| {
                r.adaptation_set_id
                    .clone()
                    .unwrap_or_else(|| r.mpd_codecs.clone())
            })
            .collect();
        set_keys.sort();
        set_keys.dedup();

        let mut adaptation_sets_xml = String::new();
        for set_key in &set_keys {
            let mut reps_xml = String::new();
            for rep in reps.iter().filter(|r| {
                let key = r
                    .adaptation_set_id
                    .clone()
                    .unwrap_or_else(|| r.mpd_codecs.clone());
                &key == set_key
            }) {
                let (timescale, segment_timeline) =
                    generate_segment_timeline(rep, fallback_seg_dur);

                fn to_dash_number_template(template: &str) -> String {
                    if template.contains("$Number") {
                        return template.to_string();
                    }
                    if template.contains("%03d$") {
                        return template.replace("%03d$", "$Number%03d$");
                    }
                    if template.contains("%03d") {
                        return template.replace("%03d", "$Number%03d$");
                    }
                    template.to_string()
                }

                let base = media_base_url.trim_end_matches('/');
                let init_path = url_encode_pathish(rep.init_segment_path.trim_start_matches('/'));
                let init = format!("{}/{}", base, init_path);
                let media_path = to_dash_number_template(&rep.segment_path_template);
                let media_path = url_encode_pathish(media_path.trim_start_matches('/'));
                let media = format!("{}/{}", base, media_path);

                reps_xml.push_str(&format!(
                    r#"      <Representation id="{}" codecs="{}" audioSamplingRate="{}" bandwidth="{}">
        <AudioChannelConfiguration schemeIdUri="urn:mpeg:dash:23003:3:audio_channel_configuration:2011" value="{}"/>
        <SegmentTemplate timescale="{}"
                         initialization="{}"
                         media="{}"
                         startNumber="1">
          <SegmentTimeline>
{}
          </SegmentTimeline>
        </SegmentTemplate>
      </Representation>
"#,
                    xml_escape_attr(&rep.name),
                    xml_escape_attr(&rep.mpd_codecs),
                    rep.sample_rate,
                    rep.bandwidth,
                    rep.channels,
                    timescale,
                    xml_escape_attr(&init),
                    xml_escape_attr(&media),
                    segment_timeline
                ));
            }

            adaptation_sets_xml.push_str(&format!(
                r#"    <AdaptationSet mimeType="audio/mp4" segmentAlignment="true" startWithSAP="1">
{}
    </AdaptationSet>
"#,
                reps_xml.trim_end()
            ));
        }

        let period_xml = format!(
            r#"  <Period id="{}" start="PT{:.3}S" duration="PT{:.3}S">
{}
  </Period>
"#,
            period.id,
            start_seconds,
            period.duration,
            adaptation_sets_xml.trim_end()
        );
        result.push_str(&period_xml);
    }

    result.trim_end().to_string()
}

fn generate_segment_timeline(
    rep: &TrackRepresentation,
    fallback_segment_duration: f64,
) -> (u32, String) {
    if let (Some(timescale), Some(entries)) = (rep.segment_timescale, &rep.segment_timeline) {
        let mut out = String::new();
        for entry in entries {
            let mut attrs = String::new();
            if let Some(t) = entry.t {
                attrs.push_str(&format!(" t=\"{}\"", t));
            }
            attrs.push_str(&format!(" d=\"{}\"", entry.d));
            if let Some(r) = entry.r {
                attrs.push_str(&format!(" r=\"{}\"", r));
            }
            out.push_str(&format!("            <S{} />\n", attrs));
        }
        return (timescale, out.trim_end().to_string());
    }

    // Fallback: constant-duration timeline at the representation timescale if known.
    let timescale = rep.segment_timescale.unwrap_or(1000u32);
    let segment_duration_ticks = (fallback_segment_duration * timescale as f64) as u64;
    let mut timeline = String::new();
    if rep.segment_count > 0 {
        timeline.push_str(&format!(
            "            <S t=\"0\" d=\"{}\" r=\"{}\" />\n",
            segment_duration_ticks,
            rep.segment_count - 1
        ));
    }
    (timescale, timeline.trim_end().to_string())
}
