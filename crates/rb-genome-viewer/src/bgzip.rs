use crate::error::{Result, ViewerError};
use crate::tracks::TrackKind;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

/// Runs synchronously (caller wraps in spawn_blocking). Writes `<input>.gz` and
/// `<input>.gz.tbi` next to the input. Returns the new `.gz` path.
pub fn bgzip_and_tabix<F: FnMut(u64, u64)>(
    input: &Path,
    kind: TrackKind,
    mut progress: F,
) -> Result<PathBuf> {
    let total = std::fs::metadata(input)?.len();
    let out_gz = {
        let mut p = input.to_path_buf();
        p.as_mut_os_string().push(".gz");
        p
    };

    // Phase 1: bgzip the input.
    {
        let src = File::open(input)?;
        let mut reader = BufReader::new(src);
        let dst = File::create(&out_gz)?;
        let mut writer = noodles_bgzf::io::Writer::new(dst);
        let mut buf = [0u8; 64 * 1024];
        let mut bytes_written: u64 = 0;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n])?;
            bytes_written += n as u64;
            progress(bytes_written, total);
        }
        writer
            .finish()
            .map_err(|e| ViewerError::IndexBuildFailed(format!("bgzf finish: {e}")))?;
    }

    // Phase 2: build .tbi next to .gz.
    build_tabix_for(&out_gz, kind)?;

    Ok(out_gz)
}

fn build_tabix_for(gz: &Path, kind: TrackKind) -> Result<()> {
    use noodles_csi::binning_index::index::header::Builder as HeaderBuilder;
    use noodles_tabix::index::Indexer;

    let header = match kind {
        TrackKind::Gff | TrackKind::Gtf => HeaderBuilder::gff().build(),
        TrackKind::Bed => HeaderBuilder::bed().build(),
    };

    let mut indexer = Indexer::default();
    indexer.set_header(header);

    match kind {
        TrackKind::Gff | TrackKind::Gtf => index_gff(gz, &mut indexer)?,
        TrackKind::Bed => index_bed(gz, &mut indexer)?,
    }

    let index = indexer.build();

    let tbi_path = {
        let mut p = gz.to_path_buf();
        p.as_mut_os_string().push(".tbi");
        p
    };

    noodles_tabix::fs::write(&tbi_path, &index)
        .map_err(|e| ViewerError::IndexBuildFailed(format!("writing .tbi: {e}")))?;

    Ok(())
}

/// Index a bgzipped GFF/GTF file, populating the tabix indexer.
///
/// We open the `.gz` with a bgzf reader (so virtual positions are available),
/// then parse each tab-delimited line to extract seqname / start / end.
/// Comment / directive lines (starting with `#`) are skipped.
fn index_gff(gz: &Path, indexer: &mut noodles_tabix::index::Indexer) -> Result<()> {
    use noodles_bgzf as bgzf;
    use noodles_core::Position;
    use noodles_csi::binning_index::index::reference_sequence::bin::Chunk;

    let f = File::open(gz)?;
    let mut reader = bgzf::io::Reader::new(f);
    let mut line = String::new();

    loop {
        let vpos_start = reader.virtual_position();
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        // Skip blank lines, comments and directives
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let vpos_end = reader.virtual_position();

        let fields: Vec<&str> = trimmed.splitn(9, '\t').collect();
        if fields.len() < 5 {
            continue;
        }
        let seqname = fields[0];
        let start: usize = fields[3]
            .parse()
            .map_err(|_| ViewerError::Parse(format!("bad GFF start: {}", fields[3])))?;
        let end: usize = fields[4]
            .parse()
            .map_err(|_| ViewerError::Parse(format!("bad GFF end: {}", fields[4])))?;

        let start_pos = Position::try_from(start)
            .map_err(|_| ViewerError::Parse(format!("invalid position: {start}")))?;
        let end_pos = Position::try_from(end)
            .map_err(|_| ViewerError::Parse(format!("invalid position: {end}")))?;

        let chunk = Chunk::new(vpos_start, vpos_end);
        indexer
            .add_record(seqname, start_pos, end_pos, chunk)
            .map_err(|e| ViewerError::IndexBuildFailed(format!("add_record: {e}")))?;
    }

    Ok(())
}

/// Index a bgzipped BED file, populating the tabix indexer.
///
/// BED coordinates are 0-based half-open; tabix/Position are 1-based. We convert:
/// start_1based = chrom_start + 1, end_1based = chrom_end (already 1-based-inclusive
/// when viewed as half-open end).
fn index_bed(gz: &Path, indexer: &mut noodles_tabix::index::Indexer) -> Result<()> {
    use noodles_bgzf as bgzf;
    use noodles_core::Position;
    use noodles_csi::binning_index::index::reference_sequence::bin::Chunk;

    let f = File::open(gz)?;
    let mut reader = bgzf::io::Reader::new(f);
    let mut line = String::new();

    loop {
        let vpos_start = reader.virtual_position();
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("track ") || trimmed.starts_with("browser ") {
            continue;
        }
        let vpos_end = reader.virtual_position();

        let fields: Vec<&str> = trimmed.splitn(4, '\t').collect();
        if fields.len() < 3 {
            continue;
        }
        let seqname = fields[0];
        // BED chrom_start is 0-based; convert to 1-based for Position
        let chrom_start: usize = fields[1]
            .parse()
            .map_err(|_| ViewerError::Parse(format!("bad BED start: {}", fields[1])))?;
        let chrom_end: usize = fields[2]
            .parse()
            .map_err(|_| ViewerError::Parse(format!("bad BED end: {}", fields[2])))?;

        let start_pos = Position::try_from(chrom_start + 1)
            .map_err(|_| ViewerError::Parse(format!("invalid position: {}", chrom_start + 1)))?;
        // BED end is exclusive, so end == last 1-based position
        let end_pos = Position::try_from(chrom_end)
            .map_err(|_| ViewerError::Parse(format!("invalid position: {chrom_end}")))?;

        let chunk = Chunk::new(vpos_start, vpos_end);
        indexer
            .add_record(seqname, start_pos, end_pos, chunk)
            .map_err(|e| ViewerError::IndexBuildFailed(format!("add_record: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::TrackKind;

    #[test]
    fn bgzip_gff_produces_gz_and_tbi() {
        let tmp = tempfile::tempdir().unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.gff3");
        let copy = tmp.path().join("tiny.gff3");
        std::fs::copy(&src, &copy).unwrap();

        let mut last_pct = 0.0_f64;
        let gz = bgzip_and_tabix(&copy, TrackKind::Gff, |done, total| {
            last_pct = done as f64 / total as f64;
        })
        .unwrap();
        assert!(gz.exists(), ".gz file not found at {}", gz.display());
        let tbi = {
            let mut p = gz.clone();
            p.as_mut_os_string().push(".tbi");
            p
        };
        assert!(tbi.exists(), "tabix index {} not found", tbi.display());
        assert!(last_pct > 0.99, "progress did not reach 100%: {last_pct}");
    }

    #[test]
    fn bgzip_bed_produces_gz_and_tbi() {
        let tmp = tempfile::tempdir().unwrap();
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiny.bed");
        let copy = tmp.path().join("tiny.bed");
        std::fs::copy(&src, &copy).unwrap();

        let gz = bgzip_and_tabix(&copy, TrackKind::Bed, |_, _| {}).unwrap();
        assert!(gz.exists(), ".gz file not found at {}", gz.display());
        let tbi = {
            let mut p = gz.clone();
            p.as_mut_os_string().push(".tbi");
            p
        };
        assert!(tbi.exists(), "tabix index {} not found", tbi.display());
    }
}
