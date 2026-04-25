use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: u64,
    pub end: u64,
}

impl Interval {
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start) + 1
    }

    pub fn is_empty(&self) -> bool {
        self.end < self.start
    }
}

#[derive(Debug, Default)]
pub struct GeneRecord {
    pub gene_id: String,
    pub transcripts: HashMap<String, Vec<Interval>>,
}

#[derive(Debug)]
pub struct ParseStats {
    pub gene_count: usize,
    pub transcript_count: usize,
    pub exon_count: usize,
}

pub fn parse_gtf(path: &Path) -> std::io::Result<(Vec<GeneRecord>, ParseStats)> {
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut by_gene: HashMap<String, GeneRecord> = HashMap::new();
    let mut exon_count: usize = 0;
    let mut transcript_count: usize = 0;
    let mut transcripts_seen: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 9 {
            continue;
        }
        if !fields[2].eq_ignore_ascii_case("exon") {
            continue;
        }
        let start: u64 = fields[3].parse().unwrap_or(0);
        let end: u64 = fields[4].parse().unwrap_or(0);
        if start == 0 || end == 0 || end < start {
            continue;
        }
        let attrs = parse_attributes(fields[8]);
        let gene_id = match attrs.get("gene_id") {
            Some(s) => s.clone(),
            None => continue,
        };
        let tx_id = attrs
            .get("transcript_id")
            .cloned()
            .unwrap_or_else(|| format!("{}.unknown", gene_id));
        let entry = by_gene
            .entry(gene_id.clone())
            .or_insert_with(|| GeneRecord {
                gene_id: gene_id.clone(),
                transcripts: HashMap::new(),
            });
        if transcripts_seen.insert((gene_id.clone(), tx_id.clone())) {
            transcript_count += 1;
        }
        entry
            .transcripts
            .entry(tx_id)
            .or_default()
            .push(Interval { start, end });
        exon_count += 1;
    }

    let gene_count = by_gene.len();
    let mut genes: Vec<GeneRecord> = by_gene.into_values().collect();
    genes.sort_by(|a, b| a.gene_id.cmp(&b.gene_id));
    Ok((
        genes,
        ParseStats {
            gene_count,
            transcript_count,
            exon_count,
        },
    ))
}

fn parse_attributes(field: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in field.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut it = part.splitn(2, char::is_whitespace);
        let key = match it.next() {
            Some(k) => k.trim().to_string(),
            None => continue,
        };
        let value_raw = match it.next() {
            Some(v) => v.trim(),
            None => continue,
        };
        let value = value_raw.trim_matches('"').to_string();
        out.insert(key, value);
    }
    out
}

/// Returns (length_union, length_longest_tx) for the gene.
/// length_union = sum of merged exon intervals across all transcripts.
/// length_longest_tx = max exon-sum among all transcripts.
pub fn gene_lengths(record: &GeneRecord) -> (u64, u64) {
    let mut all: Vec<Interval> = record
        .transcripts
        .values()
        .flat_map(|v| v.iter().copied())
        .collect();
    let union = merged_length(&mut all);
    let longest = record
        .transcripts
        .values()
        .map(|exons| exons.iter().map(|e| e.len()).sum::<u64>())
        .max()
        .unwrap_or(0);
    (union, longest)
}

fn merged_length(intervals: &mut [Interval]) -> u64 {
    if intervals.is_empty() {
        return 0;
    }
    intervals.sort_by_key(|i| (i.start, i.end));
    let mut total: u64 = 0;
    let mut cur = intervals[0];
    for iv in &intervals[1..] {
        if iv.start <= cur.end + 1 {
            if iv.end > cur.end {
                cur.end = iv.end;
            }
        } else {
            total += cur.len();
            cur = *iv;
        }
    }
    total += cur.len();
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_gtf(body: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f
    }

    #[test]
    fn parse_attrs_strips_quotes_and_handles_spaces() {
        let attrs = parse_attributes(
            "gene_id \"ENSG001\"; transcript_id \"TX1\"; gene_biotype protein_coding;",
        );
        assert_eq!(attrs.get("gene_id").unwrap(), "ENSG001");
        assert_eq!(attrs.get("transcript_id").unwrap(), "TX1");
        assert_eq!(attrs.get("gene_biotype").unwrap(), "protein_coding");
    }

    #[test]
    fn merged_length_collapses_overlaps_and_adjacents() {
        // [10..20] U [15..25] -> [10..25] = 16
        let mut iv = vec![
            Interval { start: 10, end: 20 },
            Interval { start: 15, end: 25 },
        ];
        assert_eq!(merged_length(&mut iv), 16);
        // Adjacent (touching) intervals merge: [1..10] U [11..20] -> [1..20] = 20
        let mut iv2 = vec![
            Interval { start: 1, end: 10 },
            Interval { start: 11, end: 20 },
        ];
        assert_eq!(merged_length(&mut iv2), 20);
        // Disjoint: [1..5] U [10..14] = 5+5 = 10
        let mut iv3 = vec![
            Interval { start: 1, end: 5 },
            Interval { start: 10, end: 14 },
        ];
        assert_eq!(merged_length(&mut iv3), 10);
    }

    #[test]
    fn parse_gtf_groups_exons_by_gene_and_transcript() {
        // Gene A has two transcripts:
        //   TX1: exons [1..100] + [200..300]  -> 100 + 101 = 201
        //   TX2: exons [50..150]              -> 101
        // Union: merged([1..100],[50..150],[200..300]) = [1..150] U [200..300] = 150 + 101 = 251
        // Longest tx: 201
        let gtf = "\
chr1\ttest\texon\t1\t100\t.\t+\t.\tgene_id \"A\"; transcript_id \"TX1\";\n\
chr1\ttest\texon\t200\t300\t.\t+\t.\tgene_id \"A\"; transcript_id \"TX1\";\n\
chr1\ttest\texon\t50\t150\t.\t+\t.\tgene_id \"A\"; transcript_id \"TX2\";\n";
        let f = tmp_gtf(gtf);
        let (genes, stats) = parse_gtf(f.path()).unwrap();
        assert_eq!(stats.gene_count, 1);
        assert_eq!(stats.transcript_count, 2);
        assert_eq!(stats.exon_count, 3);
        let (union_len, longest) = gene_lengths(&genes[0]);
        assert_eq!(union_len, 251);
        assert_eq!(longest, 201);
    }

    #[test]
    fn parse_gtf_skips_comments_and_non_exon_features() {
        let gtf = "\
# comment line\n\
chr1\ttest\tgene\t1\t1000\t.\t+\t.\tgene_id \"A\";\n\
chr1\ttest\ttranscript\t1\t1000\t.\t+\t.\tgene_id \"A\"; transcript_id \"TX1\";\n\
chr1\ttest\texon\t1\t100\t.\t+\t.\tgene_id \"A\"; transcript_id \"TX1\";\n";
        let f = tmp_gtf(gtf);
        let (_, stats) = parse_gtf(f.path()).unwrap();
        assert_eq!(stats.exon_count, 1);
    }

    #[test]
    fn gene_lengths_handles_single_exon_single_transcript() {
        let mut g = GeneRecord {
            gene_id: "A".into(),
            transcripts: HashMap::new(),
        };
        g.transcripts
            .insert("TX1".into(), vec![Interval { start: 1, end: 10 }]);
        let (u, lt) = gene_lengths(&g);
        assert_eq!(u, 10);
        assert_eq!(lt, 10);
    }
}
