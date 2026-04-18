//! Cancellation test using /bin/sleep as a fake STAR binary via resolver override.
//! This verifies that cancel actually kills the child process rather than waiting.

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_kills_subprocess() {
    use rb_core::binary::BinaryResolver;
    use rb_core::cancel::CancellationToken;
    use rb_core::run_event::RunEvent;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    // Point `star` at /bin/sleep via settings override.
    let tmp = tempfile::tempdir().unwrap();
    let settings = tmp.path().join("settings.json");
    let mut r = BinaryResolver::load_from(settings.clone()).unwrap();
    r.set("star", PathBuf::from("/bin/sleep")).unwrap();

    // We call the subprocess helper directly (bypasses the Module trait).
    // Re-export the helper module for tests.
    use rb_star_index::subprocess::run_star_streaming;

    let (tx, mut rx) = mpsc::channel::<RunEvent>(16);
    let token = CancellationToken::new();
    let bin = r.resolve("star").unwrap();
    let args = vec!["30".to_string()];

    let cancel_clone = token.clone();
    let handle =
        tokio::spawn(async move { run_star_streaming(&bin, &args, tx, cancel_clone).await });

    // Cancel after 200 ms; a well-behaved implementation kills /bin/sleep in well under 1s.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    token.cancel();

    let start = std::time::Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), handle)
        .await
        .expect("cancel did not complete in time")
        .expect("task panicked");
    assert!(matches!(
        result,
        Err(rb_core::module::ModuleError::Cancelled)
    ));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));

    // Drain any pending events (make sure the forwarder exits cleanly)
    drop(rx);
}
