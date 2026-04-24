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

pub fn new_sample_id() -> String {
    let short = Uuid::new_v4().to_string()[..8].to_string();
    format!("sam_{}", short)
}

/// Given a set of Fastq file names, try to detect paired-end pairs by
/// looking for the common `_R1` / `_R2` (or `_1` / `_2`) suffix convention.
/// Returns a list of (sample_name, Vec<file_name>) groups. Unpaired files
/// end up as singleton groups.
pub fn pair_fastq_names(names: &[String]) -> Vec<(String, Vec<String>)> {
    let mut by_stem: std::collections::BTreeMap<String, [Option<String>; 2]> =
        std::collections::BTreeMap::new();
    let mut singletons: Vec<(String, Vec<String>)> = Vec::new();

    for name in names {
        if let Some((stem, idx)) = split_r1r2_suffix(name) {
            let entry = by_stem.entry(stem).or_default();
            entry[idx] = Some(name.clone());
        } else {
            singletons.push((strip_fastq_ext(name), vec![name.clone()]));
        }
    }

    let mut out = Vec::new();
    for (stem, pair) in by_stem.into_iter() {
        match pair {
            [Some(r1), Some(r2)] => out.push((stem, vec![r1, r2])),
            [Some(only), None] | [None, Some(only)] => {
                out.push((strip_fastq_ext(&only), vec![only]));
            }
            [None, None] => unreachable!(),
        }
    }
    out.extend(singletons);
    out
}

/// Returns (sample_stem, 0 for R1 | 1 for R2) if the name matches the
/// `_R1` / `_R2` / `_1` / `_2` convention with any fastq-family extension.
fn split_r1r2_suffix(name: &str) -> Option<(String, usize)> {
    let stripped = strip_fastq_ext(name);
    for (suf, idx) in [("_R1", 0), ("_R2", 1), ("_1", 0), ("_2", 1)] {
        if let Some(stem) = stripped.strip_suffix(suf) {
            if !stem.is_empty() {
                return Some((stem.to_string(), idx));
            }
        }
    }
    None
}

fn strip_fastq_ext(name: &str) -> String {
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
