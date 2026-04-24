use crate::error::{Result, ViewerError};
use flate2::read::MultiGzDecoder;
use noodles_fastq as fastq;
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Hard cap on how many records we keep in memory for one session. Once
/// exceeded we refuse further streaming; the UI surfaces this as EOF-of-window
/// rather than silently truncating.
pub const SESSION_RECORD_CAP: usize = 500_000;

/// How many bytes of lookahead buffer the BufReader uses. Big buffers help
/// when the source is a gzip stream (the default 8 KiB wastes decode cycles).
const STREAM_BUF_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct FastqRecord {
    pub id: String,
    pub seq: String,
    pub plus: String,
    pub qual: String,
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    pub path: PathBuf,
    pub is_gzip: bool,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub loaded_records: usize,
    pub bytes_read: u64,
    pub total_bytes: u64,
    pub eof: bool,
    pub is_gzip: bool,
    pub cap_reached: bool,
}

#[derive(Debug, Serialize)]
pub struct ReadResult {
    pub records: Vec<FastqRecord>,
    pub loaded_records: usize,
    pub bytes_read: u64,
    pub eof: bool,
    pub cap_reached: bool,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub record_n: usize,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub scanned_through: usize,
    pub eof: bool,
    pub cap_reached: bool,
}

struct CountingReader<R> {
    inner: R,
    counter: Arc<AtomicU64>,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

fn detect_gzip(path: &Path) -> Result<bool> {
    let mut f = File::open(path)?;
    let mut magic = [0u8; 2];
    let n = f.read(&mut magic)?;
    Ok(n >= 2 && magic[0] == 0x1f && magic[1] == 0x8b)
}

fn build_reader(
    path: &Path,
    is_gzip: bool,
    counter: Arc<AtomicU64>,
) -> Result<Box<dyn BufRead + Send>> {
    let file = File::open(path)?;
    let counted = CountingReader {
        inner: file,
        counter,
    };
    if is_gzip {
        let decoder = MultiGzDecoder::new(counted);
        Ok(Box::new(BufReader::with_capacity(STREAM_BUF_SIZE, decoder)))
    } else {
        Ok(Box::new(BufReader::with_capacity(STREAM_BUF_SIZE, counted)))
    }
}

pub struct FastqSession {
    pub path: PathBuf,
    pub is_gzip: bool,
    pub total_bytes: u64,
    bytes_counter: Arc<AtomicU64>,
    inner: Mutex<SessionInner>,
}

struct SessionInner {
    reader: fastq::io::Reader<Box<dyn BufRead + Send>>,
    records: Vec<FastqRecord>,
    eof: bool,
}

impl FastqSession {
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(ViewerError::NotFound(path.to_path_buf()));
        }
        let total_bytes = std::fs::metadata(path)?.len();
        let is_gzip = detect_gzip(path)?;
        let bytes_counter = Arc::new(AtomicU64::new(0));
        let br = build_reader(path, is_gzip, bytes_counter.clone())?;
        let reader = fastq::io::Reader::new(br);
        Ok(Self {
            path: path.to_path_buf(),
            is_gzip,
            total_bytes,
            bytes_counter,
            inner: Mutex::new(SessionInner {
                reader,
                records: Vec::new(),
                eof: false,
            }),
        })
    }

    pub fn status(&self) -> Status {
        let inner = self.inner.lock().unwrap();
        Status {
            loaded_records: inner.records.len(),
            bytes_read: self.bytes_counter.load(Ordering::Relaxed),
            total_bytes: self.total_bytes,
            eof: inner.eof,
            is_gzip: self.is_gzip,
            cap_reached: inner.records.len() >= SESSION_RECORD_CAP,
        }
    }

    /// Return records `[from, from + count)`. If the loaded buffer doesn't
    /// cover the requested range, stream more records until it does, EOF hits,
    /// or the memory cap is reached.
    pub fn read(&self, from: usize, count: usize) -> Result<ReadResult> {
        let mut inner = self.inner.lock().unwrap();
        let need = from.saturating_add(count);
        while inner.records.len() < need && !inner.eof && inner.records.len() < SESSION_RECORD_CAP {
            let mut rec = fastq::Record::default();
            match inner.reader.read_record(&mut rec) {
                Ok(0) => {
                    inner.eof = true;
                    break;
                }
                Ok(_) => {
                    let desc = rec.description();
                    let plus = if desc.is_empty() {
                        "+".to_string()
                    } else {
                        format!("+{}", String::from_utf8_lossy(desc))
                    };
                    let name = String::from_utf8_lossy(rec.name()).into_owned();
                    let id = if rec.description().is_empty() {
                        format!("@{name}")
                    } else {
                        format!("@{name} {}", String::from_utf8_lossy(rec.description()))
                    };
                    inner.records.push(FastqRecord {
                        id,
                        seq: String::from_utf8_lossy(rec.sequence()).into_owned(),
                        plus,
                        qual: String::from_utf8_lossy(rec.quality_scores()).into_owned(),
                    });
                }
                Err(e) => return Err(ViewerError::Io(e)),
            }
        }

        let end = inner.records.len().min(from.saturating_add(count));
        let out = if from >= inner.records.len() {
            Vec::new()
        } else {
            inner.records[from..end].to_vec()
        };
        Ok(ReadResult {
            records: out,
            loaded_records: inner.records.len(),
            bytes_read: self.bytes_counter.load(Ordering::Relaxed),
            eof: inner.eof,
            cap_reached: inner.records.len() >= SESSION_RECORD_CAP,
        })
    }

    /// Forward-only search for records whose name contains `query`. Streams
    /// more records as needed. Scans at most `max_scan` additional records in
    /// one call so the UI can show partial progress.
    pub fn search_id(
        &self,
        query: &str,
        from: usize,
        limit: usize,
        max_scan: usize,
    ) -> Result<SearchResult> {
        let mut hits = Vec::new();
        let mut cursor = from;
        let mut scanned = 0usize;

        while hits.len() < limit && scanned < max_scan {
            // Fetch next chunk (read() will stream more if needed).
            let chunk = 1000.min(max_scan - scanned);
            let batch = self.read(cursor, chunk)?;
            if batch.records.is_empty() {
                return Ok(SearchResult {
                    hits,
                    scanned_through: cursor,
                    eof: batch.eof,
                    cap_reached: batch.cap_reached,
                });
            }
            for (i, rec) in batch.records.iter().enumerate() {
                if rec.id.contains(query) {
                    hits.push(SearchHit {
                        record_n: cursor + i,
                        id: rec.id.clone(),
                    });
                    if hits.len() == limit {
                        break;
                    }
                }
            }
            cursor += batch.records.len();
            scanned += batch.records.len();
            if batch.eof || batch.cap_reached {
                return Ok(SearchResult {
                    hits,
                    scanned_through: cursor,
                    eof: batch.eof,
                    cap_reached: batch.cap_reached,
                });
            }
        }

        Ok(SearchResult {
            hits,
            scanned_through: cursor,
            eof: false,
            cap_reached: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_sample(path: &Path, n: usize) {
        let mut f = File::create(path).unwrap();
        for i in 0..n {
            writeln!(f, "@read_{i:04} meta").unwrap();
            writeln!(f, "ACGTACGTACGTACGT").unwrap();
            writeln!(f, "+").unwrap();
            writeln!(f, "IIIIIIIIIIIIIIII").unwrap();
        }
    }

    #[test]
    fn reads_plain_stream() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.fastq");
        write_sample(&p, 10);
        let s = FastqSession::open(&p).unwrap();
        assert!(!s.is_gzip);
        let r = s.read(0, 3).unwrap();
        assert_eq!(r.records.len(), 3);
        assert_eq!(r.records[0].id, "@read_0000 meta");
        assert!(!r.eof);
        assert_eq!(r.loaded_records, 3);
    }

    #[test]
    fn streams_incrementally() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.fastq");
        write_sample(&p, 100);
        let s = FastqSession::open(&p).unwrap();
        let r1 = s.read(0, 10).unwrap();
        assert_eq!(r1.loaded_records, 10);
        let r2 = s.read(50, 10).unwrap();
        assert_eq!(r2.records.len(), 10);
        assert_eq!(r2.records[0].id, "@read_0050 meta");
        assert_eq!(r2.loaded_records, 60);
    }

    #[test]
    fn reports_eof_and_short_reads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.fastq");
        write_sample(&p, 5);
        let s = FastqSession::open(&p).unwrap();
        let r = s.read(0, 100).unwrap();
        assert_eq!(r.records.len(), 5);
        assert!(r.eof);
        assert_eq!(r.loaded_records, 5);

        let tail = s.read(10, 5).unwrap();
        assert!(tail.records.is_empty());
        assert!(tail.eof);
    }

    #[test]
    fn reads_gzipped() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.fastq.gz");
        let plain = dir.path().join("plain.fastq");
        write_sample(&plain, 25);
        let data = std::fs::read(&plain).unwrap();
        let f = File::create(&p).unwrap();
        let mut enc = GzEncoder::new(f, Compression::default());
        enc.write_all(&data).unwrap();
        enc.finish().unwrap();

        let s = FastqSession::open(&p).unwrap();
        assert!(s.is_gzip);
        let r = s.read(0, 5).unwrap();
        assert_eq!(r.records.len(), 5);
        assert_eq!(r.records[0].id, "@read_0000 meta");
        let all = s.read(0, 100).unwrap();
        assert_eq!(all.records.len(), 25);
        assert!(all.eof);
    }

    #[test]
    fn forward_search_finds_hit() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.fastq");
        write_sample(&p, 50);
        let s = FastqSession::open(&p).unwrap();
        let r = s.search_id("0042", 0, 1, 10_000).unwrap();
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].record_n, 42);
    }
}
