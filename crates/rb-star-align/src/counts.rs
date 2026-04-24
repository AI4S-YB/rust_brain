use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Strand {
    Unstranded,
    Forward,
    Reverse,
}

impl Strand {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unstranded" => Some(Self::Unstranded),
            "forward" => Some(Self::Forward),
            "reverse" => Some(Self::Reverse),
            _ => None,
        }
    }
    /// Column index in ReadsPerGene.out.tab (0=geneId, 1=unstranded, 2=forward, 3=reverse)
    pub fn column_index(self) -> usize {
        match self {
            Self::Unstranded => 1,
            Self::Forward => 2,
            Self::Reverse => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SampleSummary {
    pub n_unmapped: u64,
    pub n_multimapping: u64,
    pub n_nofeature: u64,
    pub n_ambiguous: u64,
}

#[derive(Debug)]
pub struct SampleCounts {
    pub summary: SampleSummary,
    pub genes: BTreeMap<String, u64>,
}

pub fn read_reads_per_gene(path: &Path, strand: Strand) -> std::io::Result<SampleCounts> {
    let f = std::fs::File::open(path)?;
    let reader = BufReader::new(f);
    let col = strand.column_index();
    let mut summary = SampleSummary::default();
    let mut genes: BTreeMap<String, u64> = BTreeMap::new();

    for line in reader.lines() {
        let line = line?;
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 4 {
            continue;
        }
        let id = fields[0];
        let count: u64 = fields[col].parse().unwrap_or(0);
        match id {
            "N_unmapped" => summary.n_unmapped = count,
            "N_multimapping" => summary.n_multimapping = count,
            "N_noFeature" => summary.n_nofeature = count,
            "N_ambiguous" => summary.n_ambiguous = count,
            _ => {
                genes.insert(id.to_string(), count);
            }
        }
    }
    Ok(SampleCounts { summary, genes })
}

/// Merge per-sample counts into a single matrix: rows=geneId (sorted union), cols=samples (input order).
pub fn write_counts_matrix(
    out_path: &Path,
    sample_names: &[String],
    per_sample: &[SampleCounts],
) -> std::io::Result<()> {
    let mut all_genes: BTreeMap<String, ()> = BTreeMap::new();
    for s in per_sample {
        for g in s.genes.keys() {
            all_genes.insert(g.clone(), ());
        }
    }

    let mut f = std::fs::File::create(out_path)?;
    write!(f, "gene_id")?;
    for name in sample_names {
        write!(f, "\t{}", name)?;
    }
    writeln!(f)?;

    for gene in all_genes.keys() {
        write!(f, "{}", gene)?;
        for s in per_sample {
            let c = s.genes.get(gene).copied().unwrap_or(0);
            write!(f, "\t{}", c)?;
        }
        writeln!(f)?;
    }
    Ok(())
}

pub fn union_gene_count(per_sample: &[SampleCounts]) -> usize {
    let mut all_genes: BTreeMap<String, ()> = BTreeMap::new();
    for s in per_sample {
        for g in s.genes.keys() {
            all_genes.insert(g.clone(), ());
        }
    }
    all_genes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn strand_from_str() {
        assert_eq!(Strand::from_str("unstranded"), Some(Strand::Unstranded));
        assert_eq!(Strand::from_str("forward"), Some(Strand::Forward));
        assert_eq!(Strand::from_str("reverse"), Some(Strand::Reverse));
        assert_eq!(Strand::from_str("junk"), None);
    }

    #[test]
    fn reads_summary_and_genes_unstranded() {
        let s = read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Unstranded)
            .unwrap();
        assert_eq!(s.summary.n_multimapping, 12);
        assert_eq!(s.summary.n_nofeature, 34);
        assert_eq!(s.genes.get("GENE_A"), Some(&100));
        assert_eq!(s.genes.get("GENE_B"), Some(&200));
    }

    #[test]
    fn reads_forward_column_selects_col_2() {
        let s =
            read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Forward).unwrap();
        assert_eq!(s.genes.get("GENE_A"), Some(&90));
        assert_eq!(s.summary.n_nofeature, 100);
    }

    #[test]
    fn merge_unions_genes_and_zero_fills() {
        let s1 = read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Unstranded)
            .unwrap();
        let s2 = read_reads_per_gene(&fixture("ReadsPerGene.sample2.out.tab"), Strand::Unstranded)
            .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("counts.tsv");
        write_counts_matrix(&out, &["S1".into(), "S2".into()], &[s1, s2]).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "gene_id\tS1\tS2");
        // Alphabetical: GENE_A, GENE_B, GENE_C, GENE_D
        assert_eq!(lines[1], "GENE_A\t100\t50");
        assert_eq!(lines[2], "GENE_B\t200\t0"); // S2 missing → 0
        assert_eq!(lines[3], "GENE_C\t0\t0");
        assert_eq!(lines[4], "GENE_D\t0\t300"); // S1 missing → 0
    }

    #[test]
    fn union_gene_count_counts_unique_gene_ids() {
        let s1 = read_reads_per_gene(&fixture("ReadsPerGene.sample1.out.tab"), Strand::Unstranded)
            .unwrap();
        let s2 = read_reads_per_gene(&fixture("ReadsPerGene.sample2.out.tab"), Strand::Unstranded)
            .unwrap();
        assert_eq!(union_gene_count(&[s1, s2]), 4);
    }
}
