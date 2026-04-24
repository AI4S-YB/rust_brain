// Lightweight integration test: exercise rb-genome-viewer and rb-fastq-viewer
// library-level APIs end-to-end without spinning up Tauri.

use std::path::PathBuf;

fn gv_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rb-genome-viewer/testdata")
        .join(name)
}

fn fq_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../rb-fastq-viewer/testdata")
        .join(name)
}

#[test]
fn genome_viewer_end_to_end() {
    use rb_genome_viewer::index::MemoryIndex;
    use rb_genome_viewer::reference::ReferenceHandle;
    use rb_genome_viewer::search::SearchIndex;
    use rb_genome_viewer::tracks::TrackKind;

    let (handle, meta) = ReferenceHandle::load(&gv_fixture("tiny.fa")).unwrap();
    assert_eq!(meta.chroms.len(), 2);

    let mem = MemoryIndex::load(&gv_fixture("tiny.gtf"), TrackKind::Gtf).unwrap();
    let mut search = SearchIndex::default();
    search.add_track(&"t1".to_string(), &mem);

    let hits = search.search("brca", 1);
    assert!(!hits.is_empty());

    let seq = handle
        .fetch_region(&hits[0].chrom, hits[0].start, hits[0].end)
        .unwrap();
    assert!(!seq.is_empty(), "fetched sequence should not be empty");
}

#[test]
fn fastq_viewer_end_to_end() {
    use rb_fastq_viewer::session::FastqSession;

    let session = FastqSession::open(&fq_fixture("tiny.fastq")).unwrap();
    let r = session.read(0, 5).unwrap();
    assert_eq!(r.records.len(), 5);
    let hits = session.search_id("0042", 0, 1, 10_000).unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert_eq!(hits.hits[0].record_n, 42);
}
