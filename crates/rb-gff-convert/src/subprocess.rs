use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Spawn the binary with the given argv, stream stdout/stderr lines as
/// `RunEvent::Log`, and honour cooperative cancellation. Returns Ok(()) when
/// the child exited zero; returns `ModuleError::Cancelled` if cancel fired;
/// returns `ModuleError::ToolError` with the exit status and tail of stderr
/// for non-zero exits or spawn failures.
pub async fn run_streamed(
    binary: &Path,
    argv: &[OsString],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<(), ModuleError> {
    let mut cmd = Command::new(binary);
    cmd.args(argv);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| ModuleError::ToolError(format!("failed to spawn gffread-rs: {e}")))?;

    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let tx_out = events_tx.clone();
    let tx_err = events_tx.clone();

    tokio::spawn(async move {
        let mut r = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx_out
                .send(RunEvent::Log {
                    line,
                    stream: LogStream::Stdout,
                })
                .await;
        }
    });

    tokio::spawn(async move {
        let mut r = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx_err
                .send(RunEvent::Log {
                    line,
                    stream: LogStream::Stderr,
                })
                .await;
        }
    });

    let status_or_cancel = tokio::select! {
        s = child.wait() => Ok(s),
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            Err(ModuleError::Cancelled)
        }
    };

    match status_or_cancel {
        Err(e) => Err(e),
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(ModuleError::ToolError(format!(
            "gffread-rs exited with status {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "killed".into())
        ))),
        Ok(Err(e)) => Err(ModuleError::ToolError(format!(
            "failed waiting for gffread-rs: {e}"
        ))),
    }
}
