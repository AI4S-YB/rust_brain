//! Optional end-to-end test: requires `STAR_BIN` env var pointing to a real STAR_rs binary.
//! Skipped silently when STAR_BIN is unset — CI default doesn't provide it.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn end_to_end_index_then_align() {
    let star_bin = match std::env::var("STAR_BIN") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("STAR_BIN not set; skipping");
            return;
        }
    };
    let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let tmp = tempfile::tempdir().unwrap();

    // Point the resolver at the user-supplied binary.
    let settings = tmp.path().join("settings.json");
    let mut r = rb_core::binary::BinaryResolver::load_from(settings).unwrap();
    r.set("star", std::path::PathBuf::from(&star_bin)).unwrap();

    // --- Build index ---
    let idx_dir = tmp.path().join("run_idx");
    std::fs::create_dir_all(&idx_dir).unwrap();
    let (tx, mut _rx) = tokio::sync::mpsc::channel::<rb_core::run_event::RunEvent>(64);
    let token = rb_core::cancel::CancellationToken::new();
    use rb_core::module::Module;
    let idx_mod = rb_star_index::StarIndexModule;
    let idx_params = serde_json::json!({
        "genome_fasta": data.join("chr.fa"),
        "gtf_file":     data.join("anno.gtf"),
        "threads": 2,
        "sjdb_overhang": 29,
        "genome_sa_index_nbases": 4,
    });
    let idx_result = idx_mod
        .run(&idx_params, &idx_dir, tx, token)
        .await
        .expect("index build failed");
    assert!(idx_result.output_files.iter().any(|p| p.ends_with("SA")));

    // --- Align ---
    let align_dir = tmp.path().join("run_align");
    std::fs::create_dir_all(&align_dir).unwrap();
    let (tx2, mut _rx2) = tokio::sync::mpsc::channel::<rb_core::run_event::RunEvent>(64);
    let token2 = rb_core::cancel::CancellationToken::new();
    let align_mod = rb_star_align::StarAlignModule;
    let align_params = serde_json::json!({
        "genome_dir": idx_dir,
        "reads_1": [ data.join("reads.fq") ],
        "threads": 2,
        "strand": "unstranded",
    });
    let align_result = align_mod
        .run(&align_params, &align_dir, tx2, token2)
        .await
        .expect("alignment failed");
    let matrix = align_result.summary["counts_matrix"].as_str().unwrap();
    let text = std::fs::read_to_string(matrix).unwrap();
    assert!(text.lines().count() >= 1, "counts matrix empty");
    assert!(text.lines().next().unwrap().starts_with("gene_id"));
}
