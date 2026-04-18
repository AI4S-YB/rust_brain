use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct LogFinalStats {
    pub input_reads: Option<u64>,
    pub uniquely_mapped: Option<u64>,
    pub uniquely_mapped_pct: Option<f64>,
    pub multi_mapped: Option<u64>,
    pub multi_mapped_pct: Option<f64>,
    pub unmapped: Option<u64>,
    pub unmapped_pct: Option<f64>,
}

pub fn parse(text: &str) -> LogFinalStats {
    let mut s = LogFinalStats::default();
    let mut unmapped_sum: u64 = 0;
    let mut unmapped_pct_sum: f64 = 0.0;
    let mut saw_unmapped = false;

    for line in text.lines() {
        let Some((key, val)) = line.split_once('|') else { continue; };
        let key = key.trim();
        let val = val.trim();
        match key {
            "Number of input reads" => s.input_reads = parse_u64(val),
            "Uniquely mapped reads number" => s.uniquely_mapped = parse_u64(val),
            "Uniquely mapped reads %" => s.uniquely_mapped_pct = parse_pct(val),
            "Number of reads mapped to multiple loci" => s.multi_mapped = parse_u64(val),
            "% of reads mapped to multiple loci" => s.multi_mapped_pct = parse_pct(val),
            k if k.starts_with("Number of reads unmapped:") => {
                if let Some(n) = parse_u64(val) {
                    unmapped_sum += n;
                    saw_unmapped = true;
                }
            }
            k if k.starts_with("% of reads unmapped:") => {
                if let Some(p) = parse_pct(val) {
                    unmapped_pct_sum += p;
                    saw_unmapped = true;
                }
            }
            _ => {}
        }
    }
    if saw_unmapped {
        s.unmapped = Some(unmapped_sum);
        s.unmapped_pct = Some(unmapped_pct_sum);
    }
    s
}

fn parse_u64(v: &str) -> Option<u64> {
    v.split_whitespace().next()?.replace(',', "").parse().ok()
}

fn parse_pct(v: &str) -> Option<f64> {
    v.trim_end_matches('%').trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../tests/fixtures/Log.final.out");

    #[test]
    fn parses_key_counts() {
        let s = parse(FIXTURE);
        assert_eq!(s.input_reads, Some(10_000_000));
        assert_eq!(s.uniquely_mapped, Some(9_000_000));
        assert_eq!(s.uniquely_mapped_pct, Some(90.0));
        assert_eq!(s.multi_mapped, Some(500_000));
        assert_eq!(s.multi_mapped_pct, Some(5.0));
    }

    #[test]
    fn sums_unmapped_across_categories() {
        let s = parse(FIXTURE);
        // too_many_mismatches (100k) + too_short (300k) + other (100k) = 500k
        assert_eq!(s.unmapped, Some(500_000));
        assert!((s.unmapped_pct.unwrap() - 5.0).abs() < 0.001);
    }

    #[test]
    fn tolerates_missing_fields() {
        let s = parse("    Number of input reads |\t100\n");
        assert_eq!(s.input_reads, Some(100));
        assert!(s.uniquely_mapped.is_none());
    }
}
