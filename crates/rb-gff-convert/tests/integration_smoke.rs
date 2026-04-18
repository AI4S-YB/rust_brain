//! Optional end-to-end test: requires `GFFREAD_BIN` env var pointing to a
//! gffread-rs binary. Skipped silently when unset.

#[tokio::test]
async fn end_to_end_gff3_to_gtf() {
    let gffread_bin = match std::env::var("GFFREAD_BIN") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("GFFREAD_BIN not set; skipping");
            return;
        }
    };

    let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let input = data.join("anno.gff3");
    let tmp = tempfile::tempdir().unwrap();

    // Point the resolver at the user-supplied binary.
    let settings = tmp.path().join("settings.json");
    let mut r = rb_core::binary::BinaryResolver::load_from(settings).unwrap();
    r.set("gffread-rs", std::path::PathBuf::from(&gffread_bin))
        .unwrap();

    let run_dir = tmp.path().join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let (tx, mut _rx) = tokio::sync::mpsc::channel::<rb_core::run_event::RunEvent>(64);
    let token = rb_core::cancel::CancellationToken::new();

    use rb_core::module::Module;
    let m = rb_gff_convert::GffConvertModule;
    let params = serde_json::json!({
        "input_file": input.to_string_lossy(),
        "target_format": "gtf",
    });

    let result = m.run(&params, &run_dir, tx, token).await.unwrap();
    assert_eq!(result.output_files.len(), 1);
    let out = &result.output_files[0];
    assert!(out.exists(), "output file missing: {:?}", out);
    let contents = std::fs::read_to_string(out).unwrap();
    assert!(!contents.is_empty(), "output was empty");
    assert!(
        contents.contains("transcript_id"),
        "GTF output should contain transcript_id attribute: {}",
        &contents[..contents.len().min(300)]
    );
}
