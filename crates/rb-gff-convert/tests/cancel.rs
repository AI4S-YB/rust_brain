// Unix-only: uses /bin/sleep to simulate a long-running child. The point is
// to verify run_streamed returns ModuleError::Cancelled and that the child
// is actually killed, not hung around.
#[cfg(unix)]
#[tokio::test]
async fn run_streamed_honours_cancel() {
    use rb_core::cancel::CancellationToken;
    use rb_core::module::ModuleError;
    use rb_core::run_event::RunEvent;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    let (tx, _rx) = mpsc::channel::<RunEvent>(64);
    let cancel = CancellationToken::new();
    let binary = PathBuf::from("/bin/sleep");
    let argv: Vec<OsString> = vec!["30".into()];

    let cancel_for_task = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_for_task.cancel();
    });

    let start = std::time::Instant::now();
    let res = rb_gff_convert::subprocess::run_streamed(&binary, &argv, tx, cancel).await;
    assert!(matches!(res, Err(ModuleError::Cancelled)));
    // Should return well before the 30-second sleep would have finished.
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
}
