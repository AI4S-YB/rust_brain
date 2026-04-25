use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthMode {
    Union,
    Longest,
}

impl LengthMode {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "union" => Some(Self::Union),
            "longest" => Some(Self::Longest),
            _ => None,
        }
    }

    pub fn column_name(self) -> &'static str {
        match self {
            Self::Union => "length_union",
            Self::Longest => "length_longest_tx",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Tpm,
    Fpkm,
}

impl Method {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "tpm" => Some(Self::Tpm),
            "fpkm" => Some(Self::Fpkm),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tpm => "tpm",
            Self::Fpkm => "fpkm",
        }
    }
}

pub struct CountsMatrix {
    pub samples: Vec<String>,
    pub gene_ids: Vec<String>,
    /// Row-major: rows[i][j] = counts for gene i, sample j.
    pub rows: Vec<Vec<f64>>,
}

pub fn read_counts_tsv(path: &Path) -> Result<CountsMatrix, String> {
    let f = File::open(path).map_err(|e| format!("open counts {}: {e}", path.display()))?;
    let mut reader = BufReader::new(f);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|e| format!("read counts header: {e}"))?;
    let header = header.trim_end_matches(['\r', '\n']);
    let header_fields: Vec<&str> = header.split('\t').collect();
    if header_fields.len() < 2 {
        return Err("counts matrix header must have gene id column and at least one sample".into());
    }
    let samples: Vec<String> = header_fields[1..].iter().map(|s| s.to_string()).collect();

    let mut gene_ids = Vec::new();
    let mut rows: Vec<Vec<f64>> = Vec::new();
    for (idx, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|e| format!("read counts line {}: {e}", idx + 2))?;
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != samples.len() + 1 {
            return Err(format!(
                "counts matrix line {} has {} fields, expected {}",
                idx + 2,
                fields.len(),
                samples.len() + 1
            ));
        }
        gene_ids.push(fields[0].to_string());
        let mut row = Vec::with_capacity(samples.len());
        for (j, raw) in fields[1..].iter().enumerate() {
            let v: f64 = raw.parse().map_err(|_| {
                format!(
                    "counts matrix line {} sample {} has non-numeric value '{}'",
                    idx + 2,
                    samples[j],
                    raw
                )
            })?;
            row.push(v);
        }
        rows.push(row);
    }
    Ok(CountsMatrix {
        samples,
        gene_ids,
        rows,
    })
}

/// Read a gene-length TSV with header `gene_id, length_union, length_longest_tx` (other extra
/// columns ignored). Returns a map gene_id -> chosen length (bp).
pub fn read_gene_lengths(path: &Path, mode: LengthMode) -> Result<HashMap<String, u64>, String> {
    let f = File::open(path).map_err(|e| format!("open lengths {}: {e}", path.display()))?;
    let mut reader = BufReader::new(f);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|e| format!("read lengths header: {e}"))?;
    let header = header.trim_end_matches(['\r', '\n']);
    let header_fields: Vec<&str> = header.split('\t').collect();
    let want_col = mode.column_name();
    let col_idx = header_fields
        .iter()
        .position(|c| *c == want_col)
        .ok_or_else(|| format!("lengths file is missing column '{}'", want_col))?;

    let mut out = HashMap::new();
    for (i, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|e| format!("read lengths line {}: {e}", i + 2))?;
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() <= col_idx {
            continue;
        }
        let len: u64 = fields[col_idx].parse().map_err(|_| {
            format!(
                "lengths file line {} column '{}' is not an integer: {}",
                i + 2,
                want_col,
                fields[col_idx]
            )
        })?;
        out.insert(fields[0].to_string(), len);
    }
    Ok(out)
}

#[derive(Debug, Default)]
pub struct NormalizeStats {
    pub gene_count: usize,
    pub matched_count: usize,
    pub missing_length_count: usize,
    pub zero_length_count: usize,
}

#[derive(Debug)]
pub struct NormalizedMatrix {
    pub samples: Vec<String>,
    pub gene_ids: Vec<String>,
    pub values: Vec<Vec<f64>>,
}

/// Compute TPM. Genes without a length or with length 0 are excluded.
pub fn compute_tpm(
    counts: &CountsMatrix,
    lengths: &HashMap<String, u64>,
) -> (NormalizedMatrix, NormalizeStats) {
    let (kept_gene_ids, kept_lengths_kb, stats) = filter_kept_genes(counts, lengths);
    let n_samples = counts.samples.len();
    // rate_ij = count_ij / length_kb_i
    let mut rate: Vec<Vec<f64>> = Vec::with_capacity(kept_gene_ids.len());
    let mut col_sums = vec![0.0f64; n_samples];
    for (row_idx, gene) in kept_gene_ids.iter().enumerate() {
        let original_idx = stats_kept_indices_helper(counts, gene);
        let len_kb = kept_lengths_kb[row_idx];
        let mut rate_row = Vec::with_capacity(n_samples);
        for (j, sum) in col_sums.iter_mut().enumerate() {
            let r = counts.rows[original_idx][j] / len_kb;
            rate_row.push(r);
            *sum += r;
        }
        rate.push(rate_row);
    }
    let mut values: Vec<Vec<f64>> = Vec::with_capacity(rate.len());
    for rate_row in rate.into_iter() {
        let mut out_row = Vec::with_capacity(n_samples);
        for (j, r) in rate_row.into_iter().enumerate() {
            let denom = col_sums[j];
            if denom > 0.0 {
                out_row.push(r / denom * 1.0e6);
            } else {
                out_row.push(0.0);
            }
        }
        values.push(out_row);
    }
    (
        NormalizedMatrix {
            samples: counts.samples.clone(),
            gene_ids: kept_gene_ids,
            values,
        },
        stats,
    )
}

/// Compute FPKM. Genes without a length or with length 0 are excluded.
pub fn compute_fpkm(
    counts: &CountsMatrix,
    lengths: &HashMap<String, u64>,
) -> (NormalizedMatrix, NormalizeStats) {
    let (kept_gene_ids, kept_lengths_kb, stats) = filter_kept_genes(counts, lengths);
    let n_samples = counts.samples.len();
    // total mapped reads per sample uses ALL genes (standard FPKM convention),
    // not just kept rows — gives a stable scaling factor.
    let mut totals = vec![0.0f64; n_samples];
    for row in &counts.rows {
        for (j, v) in row.iter().enumerate() {
            totals[j] += *v;
        }
    }
    let mut values = Vec::with_capacity(kept_gene_ids.len());
    for (row_idx, gene) in kept_gene_ids.iter().enumerate() {
        let original_idx = stats_kept_indices_helper(counts, gene);
        let len_kb = kept_lengths_kb[row_idx];
        let mut row_out = Vec::with_capacity(n_samples);
        for (j, total) in totals.iter().enumerate() {
            let total_m = total / 1.0e6;
            if total_m > 0.0 && len_kb > 0.0 {
                row_out.push(counts.rows[original_idx][j] / total_m / len_kb);
            } else {
                row_out.push(0.0);
            }
        }
        values.push(row_out);
    }
    (
        NormalizedMatrix {
            samples: counts.samples.clone(),
            gene_ids: kept_gene_ids,
            values,
        },
        stats,
    )
}

fn filter_kept_genes(
    counts: &CountsMatrix,
    lengths: &HashMap<String, u64>,
) -> (Vec<String>, Vec<f64>, NormalizeStats) {
    let mut kept_ids = Vec::new();
    let mut kept_kb = Vec::new();
    let mut stats = NormalizeStats {
        gene_count: counts.gene_ids.len(),
        ..Default::default()
    };
    for gene_id in &counts.gene_ids {
        match lengths.get(gene_id) {
            None => stats.missing_length_count += 1,
            Some(&0) => stats.zero_length_count += 1,
            Some(&len_bp) => {
                stats.matched_count += 1;
                kept_ids.push(gene_id.clone());
                kept_kb.push(len_bp as f64 / 1000.0);
            }
        }
    }
    (kept_ids, kept_kb, stats)
}

fn stats_kept_indices_helper(counts: &CountsMatrix, gene: &str) -> usize {
    counts
        .gene_ids
        .iter()
        .position(|g| g == gene)
        .expect("kept gene must be present in counts")
}

pub fn write_matrix(path: &Path, mat: &NormalizedMatrix) -> std::io::Result<()> {
    let mut f = File::create(path)?;
    write!(f, "gene_id")?;
    for s in &mat.samples {
        write!(f, "\t{}", s)?;
    }
    writeln!(f)?;
    for (i, gene) in mat.gene_ids.iter().enumerate() {
        write!(f, "{}", gene)?;
        for v in &mat.values[i] {
            write!(f, "\t{:.6}", v)?;
        }
        writeln!(f)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(body: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f
    }

    #[test]
    fn length_mode_from_str() {
        assert_eq!(LengthMode::from_str("union"), Some(LengthMode::Union));
        assert_eq!(LengthMode::from_str("longest"), Some(LengthMode::Longest));
        assert_eq!(LengthMode::from_str("nope"), None);
    }

    #[test]
    fn read_counts_parses_simple_matrix() {
        let f = tmp("gene_id\tS1\tS2\nA\t10\t20\nB\t30\t40\n");
        let m = read_counts_tsv(f.path()).unwrap();
        assert_eq!(m.samples, vec!["S1", "S2"]);
        assert_eq!(m.gene_ids, vec!["A", "B"]);
        assert_eq!(m.rows[0], vec![10.0, 20.0]);
    }

    #[test]
    fn read_counts_rejects_ragged_rows() {
        let f = tmp("gene_id\tS1\tS2\nA\t10\nB\t30\t40\n");
        assert!(read_counts_tsv(f.path()).is_err());
    }

    #[test]
    fn read_lengths_picks_requested_column() {
        let f = tmp("gene_id\tlength_union\tlength_longest_tx\nA\t1000\t800\nB\t2000\t1500\n");
        let union = read_gene_lengths(f.path(), LengthMode::Union).unwrap();
        assert_eq!(union.get("A"), Some(&1000));
        assert_eq!(union.get("B"), Some(&2000));
        let longest = read_gene_lengths(f.path(), LengthMode::Longest).unwrap();
        assert_eq!(longest.get("A"), Some(&800));
    }

    #[test]
    fn read_lengths_errors_on_missing_column() {
        let f = tmp("gene_id\tlength\nA\t1000\n");
        assert!(read_gene_lengths(f.path(), LengthMode::Union).is_err());
    }

    #[test]
    fn tpm_columns_sum_to_one_million() {
        // Two genes, two samples. Equal lengths so TPM proportions track counts.
        let counts = CountsMatrix {
            samples: vec!["S1".into(), "S2".into()],
            gene_ids: vec!["A".into(), "B".into()],
            rows: vec![vec![10.0, 0.0], vec![30.0, 100.0]],
        };
        let mut lengths = HashMap::new();
        lengths.insert("A".to_string(), 1000);
        lengths.insert("B".to_string(), 1000);
        let (m, stats) = compute_tpm(&counts, &lengths);
        assert_eq!(stats.matched_count, 2);
        // S1: A 25%, B 75% -> 250000, 750000
        assert!((m.values[0][0] - 250_000.0).abs() < 1e-3);
        assert!((m.values[1][0] - 750_000.0).abs() < 1e-3);
        // S2: A 0, B 1_000_000
        assert!((m.values[0][1] - 0.0).abs() < 1e-3);
        assert!((m.values[1][1] - 1_000_000.0).abs() < 1e-3);
        // Sums per column = 1e6
        for j in 0..2 {
            let col_sum: f64 = m.values.iter().map(|r| r[j]).sum();
            assert!((col_sum - 1_000_000.0).abs() < 1e-2);
        }
    }

    #[test]
    fn tpm_handles_unequal_lengths() {
        // A: count 10 / length 1kb -> rate 10
        // B: count 10 / length 2kb -> rate 5
        // Sum 15 -> A 666_666.67, B 333_333.33
        let counts = CountsMatrix {
            samples: vec!["S".into()],
            gene_ids: vec!["A".into(), "B".into()],
            rows: vec![vec![10.0], vec![10.0]],
        };
        let mut lengths = HashMap::new();
        lengths.insert("A".to_string(), 1000);
        lengths.insert("B".to_string(), 2000);
        let (m, _) = compute_tpm(&counts, &lengths);
        assert!((m.values[0][0] - 666_666.666_666_7).abs() < 0.1);
        assert!((m.values[1][0] - 333_333.333_333_3).abs() < 0.1);
    }

    #[test]
    fn fpkm_basic_formula() {
        // counts S1: A=1_000_000 (1Mb counts total), length 1kb -> FPKM = 1e6 / (1e6/1e6) / 1 = 1e6
        let counts = CountsMatrix {
            samples: vec!["S1".into()],
            gene_ids: vec!["A".into()],
            rows: vec![vec![1_000_000.0]],
        };
        let mut lengths = HashMap::new();
        lengths.insert("A".to_string(), 1000);
        let (m, _) = compute_fpkm(&counts, &lengths);
        assert!((m.values[0][0] - 1_000_000.0).abs() < 1e-3);
    }

    #[test]
    fn missing_lengths_are_excluded_and_counted() {
        let counts = CountsMatrix {
            samples: vec!["S1".into()],
            gene_ids: vec!["A".into(), "B".into(), "C".into()],
            rows: vec![vec![10.0], vec![20.0], vec![30.0]],
        };
        let mut lengths = HashMap::new();
        lengths.insert("A".to_string(), 1000);
        lengths.insert("B".to_string(), 0);
        // C absent
        let (m, stats) = compute_tpm(&counts, &lengths);
        assert_eq!(stats.matched_count, 1);
        assert_eq!(stats.zero_length_count, 1);
        assert_eq!(stats.missing_length_count, 1);
        assert_eq!(m.gene_ids, vec!["A"]);
    }
}
