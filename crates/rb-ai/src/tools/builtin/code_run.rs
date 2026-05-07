//! code_run — execute Python/R/Shell scripts inside the sandbox.
//!
//! Runtime is selected by `AgentConfig.code_run.runtime`. `pixi` is the
//! default; `system` falls back to PATH-resolved python/Rscript/bash;
//! `custom` runs `<custom_command> <interp> <script>`.
//!
//! Streaming: stderr/stdout lines are surfaced via the agent's RunEvent
//! channel — wired by execute.rs in Phase 4. This module only owns the
//! one-shot blocking execution path used by tests.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: code_run_def(),
        executor: std::sync::Arc::new(CodeRunExec),
    });
}

fn code_run_def() -> ToolDef {
    ToolDef {
        name: "code_run".into(),
        description: "Run a Python/R/shell script. cwd defaults to <project>/sandbox/.".into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "language": {"type": "string", "enum": ["python", "r", "shell"]},
                "code": {"type": "string"},
                "cwd": {"type": "string"},
                "timeout_secs": {"type": "integer", "default": 600}
            },
            "required": ["language", "code"]
        }),
    }
}

struct CodeRunExec;
#[async_trait]
impl ToolExecutor for CodeRunExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("language required".into()))?;
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("code required".into()))?;
        let cwd: PathBuf = args
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .ok_or_else(|| ToolError::InvalidArgs("cwd required".into()))?;
        let timeout = Duration::from_secs(
            args.get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(600),
        );
        std::fs::create_dir_all(&cwd).map_err(|e| ToolError::Execution(e.to_string()))?;
        let (script_name, interp) = match language {
            "python" => ("agent_run.py", "python"),
            "r" => ("agent_run.R", "Rscript"),
            "shell" => ("agent_run.sh", "bash"),
            other => {
                return Err(ToolError::InvalidArgs(format!(
                    "unsupported language: {other}"
                )))
            }
        };
        let script = cwd.join(script_name);
        std::fs::write(&script, code).map_err(|e| ToolError::Execution(e.to_string()))?;

        let mut cmd = Command::new(interp);
        cmd.arg(&script).current_dir(&cwd);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        crate::subprocess::harden_for_gui(&mut cmd);

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::Execution(format!("spawn {interp}: {e}")))?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let stdout_task = tokio::spawn(async move {
            let mut buf = String::new();
            let mut r = BufReader::new(stdout);
            let mut line = String::new();
            while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                buf.push_str(&line);
                line.clear();
                if buf.len() > 256 * 1024 {
                    break;
                }
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = String::new();
            let mut r = BufReader::new(stderr);
            let mut line = String::new();
            while r.read_line(&mut line).await.unwrap_or(0) > 0 {
                buf.push_str(&line);
                line.clear();
                if buf.len() > 256 * 1024 {
                    break;
                }
            }
            buf
        });

        let exit = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(ToolError::Execution(format!("wait: {e}"))),
            Err(_) => {
                let _ = child.kill().await;
                return Err(ToolError::Execution("timeout".into()));
            }
        };
        let stdout = stdout_task.await.unwrap_or_default();
        let stderr = stderr_task.await.unwrap_or_default();
        Ok(ToolOutput::Value(json!({
            "exit_code": exit.code(),
            "stdout": stdout,
            "stderr": stderr
        })))
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn ctx(root: &std::path::Path) -> ToolContext<'static> {
        let project = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", root).unwrap(),
        ))));
        let runner = Box::leak(Box::new(std::sync::Arc::new(rb_core::runner::Runner::new(
            project.clone(),
        ))));
        let binres = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::with_defaults_at(root.join("binaries.json")),
        ))));
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
            memory: None,
            session_id: None,
            project_root: None,
            ask_user_tx: None,
        }
    }

    #[tokio::test]
    async fn shell_echo_returns_stdout() {
        let tmp = tempdir().unwrap();
        let exec = CodeRunExec;
        let out = exec
            .execute(
                &json!({
                    "language": "shell",
                    "code": "echo hello-world",
                    "cwd": tmp.path().display().to_string(),
                    "timeout_secs": 5
                }),
                ctx(tmp.path()),
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["exit_code"], 0);
        assert!(v["stdout"].as_str().unwrap().contains("hello-world"));
    }

    #[tokio::test]
    async fn timeout_kills_long_running_script() {
        let tmp = tempdir().unwrap();
        let exec = CodeRunExec;
        let r = exec
            .execute(
                &json!({
                    "language": "shell",
                    "code": "sleep 10",
                    "cwd": tmp.path().display().to_string(),
                    "timeout_secs": 1
                }),
                ctx(tmp.path()),
            )
            .await;
        assert!(matches!(r, Err(ToolError::Execution(s)) if s.contains("timeout")));
    }
}
