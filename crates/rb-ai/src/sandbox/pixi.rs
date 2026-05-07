//! Wrap pixi for sandbox-scoped Python/R/shell execution.
//!
//! Detection is best-effort: `which pixi` (or `where` on Windows). If pixi is
//! missing we return a structured error so the agent can ask the user to
//! install it (https://pixi.sh).
//!
//! `init_if_needed` runs `pixi init` once per sandbox dir.
//! `build_command` returns a `tokio::process::Command` ready to spawn —
//! the caller wires stdin/stdout/stderr and `crate::subprocess::harden_for_gui`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::AiError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Lang {
    Python,
    R,
    Shell,
}

#[derive(Debug, Clone)]
pub struct PixiRuntime {
    pub bin: PathBuf,
}

impl PixiRuntime {
    pub fn detect() -> Result<Self, AiError> {
        let bin_name = if cfg!(windows) { "pixi.exe" } else { "pixi" };
        let path = which::which(bin_name).map_err(|_| {
            AiError::Tool(
                "pixi not found in PATH; install from https://pixi.sh and retry".to_string(),
            )
        })?;
        Ok(Self { bin: path })
    }

    pub async fn init_if_needed(&self, sandbox_dir: &Path) -> Result<(), AiError> {
        let pixi_toml = sandbox_dir.join("pixi.toml");
        if pixi_toml.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(sandbox_dir)?;
        let mut cmd = Command::new(&self.bin);
        cmd.arg("init").current_dir(sandbox_dir);
        crate::subprocess::harden_for_gui(&mut cmd);
        let out = cmd
            .output()
            .await
            .map_err(|e| AiError::Tool(format!("pixi init failed to spawn: {e}")))?;
        if !out.status.success() {
            return Err(AiError::Tool(format!(
                "pixi init exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    /// Build a runnable Command. Caller handles stdin/stdout/stderr piping.
    pub fn build_command(&self, sandbox_dir: &Path, lang: Lang, script_path: &Path) -> Command {
        let interp = match lang {
            Lang::Python => "python",
            Lang::R => "Rscript",
            Lang::Shell => "bash",
        };
        let mut cmd = Command::new(&self.bin);
        cmd.arg("run")
            .arg("--manifest-path")
            .arg(sandbox_dir.join("pixi.toml"))
            .arg("--")
            .arg(interp)
            .arg(script_path);
        cmd.current_dir(sandbox_dir);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_serializes_as_pascal_case() {
        let v = serde_json::to_value(Lang::Python).unwrap();
        assert_eq!(v.as_str(), Some("Python"));
    }

    #[test]
    fn detect_returns_structured_error_when_pixi_absent() {
        // Force PATH=empty; we can't reliably test "found" cross-CI, so test absent.
        let saved = std::env::var_os("PATH");
        std::env::set_var("PATH", "");
        let r = PixiRuntime::detect();
        if let Some(p) = saved {
            std::env::set_var("PATH", p);
        }
        match r {
            Err(AiError::Tool(msg)) => assert!(msg.contains("pixi")),
            other => panic!("expected Tool err, got {other:?}"),
        }
    }
}
