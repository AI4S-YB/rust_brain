use rb_core::cancel::CancellationToken;
use rb_core::module::ModuleError;
use rb_core::run_event::{LogStream, RunEvent};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

pub async fn run_star_streaming(
    bin: &PathBuf,
    args: &[String],
    events_tx: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
) -> Result<std::process::ExitStatus, ModuleError> {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    rb_core::subprocess::harden_for_gui(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| {
        ModuleError::ToolError(format!("failed to spawn {}: {}", bin.display(), e,))
    })?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let tx_out = events_tx.clone();
    let tx_err = events_tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_out
                .send(RunEvent::Log {
                    line,
                    stream: LogStream::Stdout,
                })
                .await;
        }
    });
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx_err
                .send(RunEvent::Log {
                    line,
                    stream: LogStream::Stderr,
                })
                .await;
        }
    });

    tokio::select! {
        status = child.wait() => status.map_err(|e| ModuleError::ToolError(e.to_string())),
        _ = cancel.cancelled() => {
            let _ = child.kill().await;
            Err(ModuleError::Cancelled)
        }
    }
}
