use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SampleRecord {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// InputRecord ids. For paired-end: 2 entries (R1, R2 in order).
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub paired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SamplePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ReadPairPattern {
    pub r1: String,
    pub r2: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SamplePairPreview {
    pub name: String,
    pub inputs: Vec<String>,
    pub paired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<ReadPairPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadPairNameHit {
    pub sample_name: String,
    pub pattern_index: usize,
    pub read_index: usize,
    pub pattern: ReadPairPattern,
}

pub fn new_sample_id() -> String {
    let short = Uuid::new_v4().to_string()[..8].to_string();
    format!("sam_{}", short)
}

pub fn default_read_pair_patterns() -> Vec<ReadPairPattern> {
    vec![
        ReadPairPattern {
            r1: "_R1_001".into(),
            r2: "_R2_001".into(),
        },
        ReadPairPattern {
            r1: "_R1".into(),
            r2: "_R2".into(),
        },
        ReadPairPattern {
            r1: ".R1".into(),
            r2: ".R2".into(),
        },
        ReadPairPattern {
            r1: "-R1".into(),
            r2: "-R2".into(),
        },
        ReadPairPattern {
            r1: "R1".into(),
            r2: "R2".into(),
        },
        ReadPairPattern {
            r1: "_1".into(),
            r2: "_2".into(),
        },
        ReadPairPattern {
            r1: ".1".into(),
            r2: ".2".into(),
        },
        ReadPairPattern {
            r1: "-1".into(),
            r2: "-2".into(),
        },
    ]
}

/// Given a set of Fastq file names, try to detect paired-end pairs by
/// looking for the common R1/R2 marker conventions.
/// Returns a list of (sample_name, Vec<file_name>) groups. Unpaired files
/// end up as singleton groups.
pub fn pair_fastq_names(names: &[String]) -> Vec<(String, Vec<String>)> {
    pair_fastq_names_with_patterns(names, &default_read_pair_patterns())
}

pub fn pair_fastq_names_with_patterns(
    names: &[String],
    patterns: &[ReadPairPattern],
) -> Vec<(String, Vec<String>)> {
    let patterns = normalize_patterns(patterns);
    let mut by_stem: std::collections::BTreeMap<(usize, String), PairBucket<String>> =
        std::collections::BTreeMap::new();
    let mut singletons: Vec<(String, Vec<String>)> = Vec::new();

    for name in names {
        if let Some(hit) = classify_read_pair_name(name, &patterns) {
            let entry = by_stem
                .entry((hit.pattern_index, hit.sample_name.clone()))
                .or_insert_with(|| PairBucket {
                    name: hit.sample_name.clone(),
                    reads: [None, None],
                });
            if entry.reads[hit.read_index].is_none() {
                entry.reads[hit.read_index] = Some(name.clone());
            } else {
                singletons.push((strip_fastq_ext(name), vec![name.clone()]));
            }
        } else {
            singletons.push((strip_fastq_ext(name), vec![name.clone()]));
        }
    }

    let mut out = Vec::new();
    for (_, bucket) in by_stem.into_iter() {
        match bucket.reads {
            [Some(r1), Some(r2)] => out.push((bucket.name, vec![r1, r2])),
            [Some(only), None] | [None, Some(only)] => {
                out.push((strip_fastq_ext(&only), vec![only]));
            }
            [None, None] => unreachable!(),
        }
    }
    out.extend(singletons);
    out
}

pub fn classify_read_pair_name(
    name: &str,
    patterns: &[ReadPairPattern],
) -> Option<ReadPairNameHit> {
    let stripped = strip_fastq_ext(name);
    let patterns = normalize_patterns(patterns);
    for (pattern_index, pattern) in patterns.iter().enumerate() {
        if let Some(sample_name) = sample_name_without_marker(&stripped, &pattern.r1) {
            return Some(ReadPairNameHit {
                sample_name,
                pattern_index,
                read_index: 0,
                pattern: pattern.clone(),
            });
        }
        if let Some(sample_name) = sample_name_without_marker(&stripped, &pattern.r2) {
            return Some(ReadPairNameHit {
                sample_name,
                pattern_index,
                read_index: 1,
                pattern: pattern.clone(),
            });
        }
    }
    None
}

pub fn strip_fastq_ext(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let candidates = [
        ".fastq.gz",
        ".fq.gz",
        ".fastq.bz2",
        ".fq.bz2",
        ".fastq",
        ".fq",
    ];
    for suf in candidates {
        if lower.ends_with(suf) {
            return name[..name.len() - suf.len()].to_string();
        }
    }
    name.to_string()
}

fn normalize_patterns(patterns: &[ReadPairPattern]) -> Vec<ReadPairPattern> {
    let mut out: Vec<ReadPairPattern> = patterns
        .iter()
        .filter_map(|p| {
            let r1 = p.r1.trim();
            let r2 = p.r2.trim();
            if r1.is_empty() || r2.is_empty() {
                return None;
            }
            Some(ReadPairPattern {
                r1: r1.to_string(),
                r2: r2.to_string(),
            })
        })
        .collect();
    if out.is_empty() {
        out = default_read_pair_patterns();
    }
    out
}

fn sample_name_without_marker(stem: &str, marker: &str) -> Option<String> {
    let lower_stem = stem.to_ascii_lowercase();
    let lower_marker = marker.to_ascii_lowercase();
    let mut start = 0;
    while let Some(rel) = lower_stem[start..].find(&lower_marker) {
        let idx = start + rel;
        let end = idx + marker.len();
        if marker_boundary_ok(stem, marker, idx, end) {
            let mut sample = String::with_capacity(stem.len().saturating_sub(marker.len()));
            sample.push_str(&stem[..idx]);
            sample.push_str(&stem[end..]);
            let sample = clean_sample_name(&sample);
            if !sample.is_empty() {
                return Some(sample);
            }
        }
        start = end;
    }
    None
}

fn marker_boundary_ok(stem: &str, marker: &str, idx: usize, end: usize) -> bool {
    let first = marker.as_bytes().first().copied();
    let last = marker.as_bytes().last().copied();

    if first.is_some_and(|b| b.is_ascii_alphanumeric()) && idx > 0 {
        if stem.as_bytes()[idx - 1].is_ascii_alphanumeric() {
            return false;
        }
    }
    if last.is_some_and(|b| b.is_ascii_alphanumeric()) && end < stem.len() {
        if stem.as_bytes()[end].is_ascii_alphanumeric() {
            return false;
        }
    }
    true
}

fn clean_sample_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_delim = false;
    for c in raw.chars() {
        let is_delim = matches!(c, '_' | '-' | '.' | ' ');
        if is_delim {
            if !prev_delim {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        prev_delim = is_delim;
    }
    out.trim_matches(|c| matches!(c, '_' | '-' | '.' | ' '))
        .to_string()
}

#[derive(Debug, Clone)]
struct PairBucket<T> {
    name: String,
    reads: [Option<T>; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_id_has_prefix() {
        let id = new_sample_id();
        assert!(id.starts_with("sam_"));
    }

    #[test]
    fn pairs_r1_r2_by_stem() {
        let names = vec![
            "sample_A_R1.fastq.gz".to_string(),
            "sample_A_R2.fastq.gz".to_string(),
            "sample_B_R1.fq".to_string(),
            "sample_B_R2.fq".to_string(),
        ];
        let groups = pair_fastq_names(&names);
        assert_eq!(groups.len(), 2);
        let a = groups.iter().find(|(k, _)| k == "sample_A").unwrap();
        assert_eq!(a.1.len(), 2);
        assert_eq!(a.1[0], "sample_A_R1.fastq.gz");
        assert_eq!(a.1[1], "sample_A_R2.fastq.gz");
    }

    #[test]
    fn pairs_underscore_1_2_convention() {
        let names = vec![
            "reads_1.fastq.gz".to_string(),
            "reads_2.fastq.gz".to_string(),
        ];
        let groups = pair_fastq_names(&names);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "reads");
        assert_eq!(groups[0].1.len(), 2);
    }

    #[test]
    fn pairs_prefix_r1_r2_with_shared_suffix() {
        let names = vec!["R1.raw.fastq.gz".to_string(), "R2.raw.fastq.gz".to_string()];
        let groups = pair_fastq_names(&names);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "raw");
        assert_eq!(groups[0].1, vec!["R1.raw.fastq.gz", "R2.raw.fastq.gz"]);
    }

    #[test]
    fn pairs_lane_suffix_convention() {
        let names = vec![
            "sample_A_R1_001.fastq.gz".to_string(),
            "sample_A_R2_001.fastq.gz".to_string(),
        ];
        let groups = pair_fastq_names(&names);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "sample_A");
        assert_eq!(groups[0].1.len(), 2);
    }

    #[test]
    fn supports_custom_read_markers() {
        let names = vec![
            "sampleA.read1.fq.gz".to_string(),
            "sampleA.read2.fq.gz".to_string(),
        ];
        let patterns = vec![ReadPairPattern {
            r1: ".read1".into(),
            r2: ".read2".into(),
        }];
        let groups = pair_fastq_names_with_patterns(&names, &patterns);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "sampleA");
        assert_eq!(groups[0].1.len(), 2);
    }

    #[test]
    fn bare_r1_marker_requires_boundaries() {
        let hit = classify_read_pair_name("BR1.raw.fastq.gz", &default_read_pair_patterns());
        assert!(hit.is_none());
    }

    #[test]
    fn unpaired_stays_singleton() {
        let names = vec!["lonely.fastq".to_string()];
        let groups = pair_fastq_names(&names);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 1);
    }

    #[test]
    fn half_pair_demotes_to_singleton() {
        let names = vec!["half_R1.fastq.gz".to_string()];
        let groups = pair_fastq_names(&names);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].1.len(), 1);
    }
}
