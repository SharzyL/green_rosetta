use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorConfig {
    pub streaming: Option<StreamingConfig>,
    pub audio: Option<AudioConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    pub segment_duration: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default)]
    pub profiles: Vec<AudioProfile>,
    // Accept both spellings; config.toml currently uses "adaption_sets".
    #[serde(default, alias = "adaptation_sets")]
    pub adaption_sets: Vec<AdaptionSetConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioProfile {
    pub name: String,
    /// Optional grouping identifier for DASH AdaptationSet generation/validation.
    /// When set, segment/timeline equality checks are only performed within the same set.
    #[serde(
        default,
        alias = "adaption_set_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub adaptation_set_id: Option<String>,
    pub codec: String,
    pub bitrate: String,
    pub sample_rate: u32,
    pub channels: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptionSetConfig {
    pub codec: String,
    #[serde(default)]
    pub bitrates: Vec<String>,
    pub bitrate: Option<String>,
    pub sample_rate: u32,
    pub channels: u8,
}

#[derive(Debug, Clone)]
pub struct AdaptionSetPlan {
    pub id: String,
    pub codec: String,
    pub bitrates: Vec<String>,
    pub sample_rate: u32,
    pub channels: u8,
}

impl GeneratorConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read config {}: {}", path.display(), e))?;
        let cfg = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse config {}: {}", path.display(), e))?;
        Ok(cfg)
    }

    pub fn segment_duration_seconds(&self) -> f64 {
        self.streaming
            .as_ref()
            .and_then(|s| s.segment_duration)
            .unwrap_or(6.0)
    }

    /// Profiles used for encoding and metadata generation.
    ///
    /// - If `audio.adaption_sets` is present, expand each set into multiple bitrate profiles and
    ///   tag them with `adaptation_set_id` so downstream code can group/validate correctly.
    /// - Otherwise fall back to legacy `audio.profiles`.
    pub fn encoding_profiles(&self) -> Result<Vec<AudioProfile>> {
        let audio = match &self.audio {
            Some(a) => a,
            None => {
                return Ok(vec![AudioProfile {
                    name: "aac_128k".to_string(),
                    adaptation_set_id: None,
                    codec: "aac".to_string(),
                    bitrate: "128k".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                }]);
            }
        };

        if !audio.adaption_sets.is_empty() {
            let sets = self.adaption_sets();
            validate_adaption_sets(&sets)?;

            let mut profiles = Vec::new();
            let mut used_names: std::collections::HashSet<String> =
                std::collections::HashSet::new();

            for set in sets {
                // Preserve order but avoid duplicates like ["128k", "128k"].
                let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
                for br in set.bitrates {
                    if !seen.insert(br.clone()) {
                        continue;
                    }
                    let base = format!("{}_{}", set.id, sanitize_component(&br));
                    let name = make_unique_name(base, &mut used_names);

                    profiles.push(AudioProfile {
                        name,
                        adaptation_set_id: Some(set.id.clone()),
                        codec: set.codec.clone(),
                        bitrate: br,
                        sample_rate: set.sample_rate,
                        channels: set.channels,
                    });
                }
            }

            return Ok(profiles);
        }

        // Legacy config path.
        if !audio.profiles.is_empty() {
            return Ok(audio.profiles.clone());
        }

        Ok(vec![AudioProfile {
            name: "aac_128k".to_string(),
            adaptation_set_id: None,
            codec: "aac".to_string(),
            bitrate: "128k".to_string(),
            sample_rate: 48000,
            channels: 2,
        }])
    }

    pub fn adaption_sets(&self) -> Vec<AdaptionSetPlan> {
        let audio = match &self.audio {
            Some(a) => a,
            None => {
                return vec![AdaptionSetPlan {
                    id: "as0_aac".to_string(),
                    codec: "aac".to_string(),
                    bitrates: vec!["128k".to_string()],
                    sample_rate: 48000,
                    channels: 2,
                }];
            }
        };

        if !audio.adaption_sets.is_empty() {
            return audio
                .adaption_sets
                .iter()
                .enumerate()
                .map(|(idx, set)| {
                    let mut bitrates = set.bitrates.clone();
                    if let Some(one) = &set.bitrate {
                        bitrates.push(one.clone());
                    }
                    AdaptionSetPlan {
                        id: format!("as{}_{}", idx, set.codec),
                        codec: set.codec.clone(),
                        bitrates,
                        sample_rate: set.sample_rate,
                        channels: set.channels,
                    }
                })
                .collect();
        }

        // Legacy config: flat profiles list -> group into sets by codec/sample_rate/channels.
        if !audio.profiles.is_empty() {
            let mut groups: std::collections::BTreeMap<(String, u32, u8), Vec<String>> =
                std::collections::BTreeMap::new();
            for p in &audio.profiles {
                groups
                    .entry((p.codec.clone(), p.sample_rate, p.channels))
                    .or_default()
                    .push(p.bitrate.clone());
            }

            return groups
                .into_iter()
                .enumerate()
                .map(
                    |(idx, ((codec, sample_rate, channels), bitrates))| AdaptionSetPlan {
                        id: format!("as{}_{}", idx, codec),
                        codec,
                        bitrates,
                        sample_rate,
                        channels,
                    },
                )
                .collect();
        }

        vec![AdaptionSetPlan {
            id: "as0_aac".to_string(),
            codec: "aac".to_string(),
            bitrates: vec!["128k".to_string()],
            sample_rate: 48000,
            channels: 2,
        }]
    }
}

fn sanitize_component(s: &str) -> String {
    s.trim()
        .replace([' ', '/', '\\'], "_")
}

fn make_unique_name(base: String, used: &mut std::collections::HashSet<String>) -> String {
    if !used.contains(&base) {
        used.insert(base.clone());
        return base;
    }
    let mut i = 2;
    loop {
        let candidate = format!("{}_{}", base, i);
        if !used.contains(&candidate) {
            used.insert(candidate.clone());
            return candidate;
        }
        i += 1;
    }
}

pub fn bitrate_to_bps(bitrate: &str) -> Result<u32> {
    // Accept forms like "128k", "160k", "1M", or raw numbers (bps).
    let s = bitrate.trim();
    if s.is_empty() {
        return Err(anyhow!("Empty bitrate"));
    }
    let (num, mult) = match s.chars().last().unwrap() {
        'k' | 'K' => (&s[..s.len() - 1], 1000u32),
        'm' | 'M' => (&s[..s.len() - 1], 1_000_000u32),
        _ => (s, 1u32),
    };
    let v = u32::from_str(num.trim()).map_err(|e| anyhow!("Invalid bitrate {}: {}", bitrate, e))?;
    Ok(v.saturating_mul(mult))
}

pub fn mpd_codecs_for_profile(profile: &AudioProfile) -> Result<String> {
    // These are the common codec strings for audio in MP4 for DASH.
    // Note: browser support differs significantly for Opus-in-MP4.
    match profile.codec.as_str() {
        "aac" => Ok("mp4a.40.2".to_string()),
        "libopus" | "opus" => Ok("opus".to_string()),
        other => Err(anyhow!(
            "Unsupported codec in profile {}: {}",
            profile.name,
            other
        )),
    }
}

pub fn ffmpeg_codec_for_profile(profile: &AudioProfile) -> Result<&'static str> {
    match profile.codec.as_str() {
        "aac" => Ok("aac"),
        "libopus" | "opus" => Ok("libopus"),
        other => Err(anyhow!(
            "Unsupported codec in profile {}: {}",
            profile.name,
            other
        )),
    }
}

pub fn validate_profiles(profiles: &[AudioProfile]) -> Result<()> {
    if profiles.is_empty() {
        return Err(anyhow!("No audio profiles configured"));
    }
    for p in profiles {
        if p.name.trim().is_empty() {
            return Err(anyhow!("Audio profile name cannot be empty"));
        }
        let _ = ffmpeg_codec_for_profile(p)?;
        let _ = mpd_codecs_for_profile(p)?;
        let _ = bitrate_to_bps(&p.bitrate)?;
        if p.sample_rate == 0 {
            return Err(anyhow!("Invalid sample_rate in profile {}", p.name));
        }
        if p.channels == 0 {
            return Err(anyhow!("Invalid channels in profile {}", p.name));
        }
    }
    Ok(())
}

pub fn validate_adaption_sets(sets: &[AdaptionSetPlan]) -> Result<()> {
    if sets.is_empty() {
        return Err(anyhow!("No audio adaption sets configured"));
    }

    for set in sets {
        if set.id.trim().is_empty() {
            return Err(anyhow!("Adaption set id cannot be empty"));
        }
        // Validate codec + each bitrate.
        let dummy_profile = AudioProfile {
            name: set.id.clone(),
            adaptation_set_id: None,
            codec: set.codec.clone(),
            bitrate: "128k".to_string(),
            sample_rate: set.sample_rate,
            channels: set.channels,
        };
        let _ = ffmpeg_codec_for_profile(&dummy_profile)?;
        let _ = mpd_codecs_for_profile(&dummy_profile)?;

        if set.sample_rate == 0 {
            return Err(anyhow!("Invalid sample_rate in adaption set {}", set.id));
        }
        if set.channels == 0 {
            return Err(anyhow!("Invalid channels in adaption set {}", set.id));
        }
        if set.bitrates.is_empty() {
            return Err(anyhow!(
                "No bitrates configured for adaption set {}",
                set.id
            ));
        }
        for br in &set.bitrates {
            let _ = bitrate_to_bps(br)?;
        }
    }

    Ok(())
}
