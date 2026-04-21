//! Spawn an external command, stream stdout/stderr lines as RunEvent::Log,
//! honour cancellation. Returns Ok if exit zero; ToolError otherwise.
//!
//! Same shape as `rb-gff-convert::subprocess::run_streamed` — kept duplicated
//! intentionally because the two crates have no shared subprocess crate.
//! When a third copy appears, extract to a shared helper.

use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub async fn run_streamed(
    binary: &std::path::Path,
    argv_after_binary: &[String],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<i32, ModuleError> {
    let mut cmd = Command::new(binary);
    cmd.args(argv_after_binary);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| ModuleError::ToolError(format!("failed to spawn {}: {e}", binary.display())))?;

    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let tx_out = events_tx.clone();
    let tx_err = events_tx.clone();

    tokio::spawn(async move {
        let mut r = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx_out.send(RunEvent::Log { line, stream: LogStream::Stdout }).await;
        }
    });
    tokio::spawn(async move {
        let mut r = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = r.next_line().await {
            let _ = tx_err.send(RunEvent::Log { line, stream: LogStream::Stderr }).await;
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
        Ok(Ok(status)) if status.success() => Ok(status.code().unwrap_or(0)),
        Ok(Ok(status)) => Err(ModuleError::ToolError(format!(
            "process exited with status {}",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "killed".into())
        ))),
        Ok(Err(e)) => Err(ModuleError::ToolError(format!("failed waiting on child: {e}"))),
    }
}
