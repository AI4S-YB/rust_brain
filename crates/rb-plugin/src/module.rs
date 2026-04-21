//! `ExternalToolModule` — implements rb_core::module::Module from a manifest.
//!
//! The module owns:
//!   * an `Arc<PluginManifest>` (cheap clone, shared across runs)
//!   * a resolved `binary_path` snapshot taken at construction time
//!
//! Output discovery: after a successful exit, glob `manifest.outputs.patterns`
//! relative to the resolved `output_dir` and add matches to `output_files`.

use crate::argv::build_argv;
use crate::manifest::{ParamType, PluginManifest};
use crate::schema::derive_json_schema;
use crate::subprocess::run_streamed;
use crate::validate::validate_against_manifest;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct ExternalToolModule {
    manifest: Arc<PluginManifest>,
    binary_path: PathBuf,
    schema_cache: serde_json::Value,
}

impl ExternalToolModule {
    pub fn new(manifest: Arc<PluginManifest>, binary_path: PathBuf) -> Self {
        let schema_cache = derive_json_schema(&manifest);
        Self { manifest, binary_path, schema_cache }
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }
}

#[async_trait::async_trait]
impl Module for ExternalToolModule {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(self.schema_cache.clone())
    }

    fn ai_hint(&self, lang: &str) -> String {
        let s = self.manifest.strings.as_ref();
        let from_strings = match lang {
            "zh" => s.and_then(|s| s.ai_hint_zh.clone()),
            _ => s.and_then(|s| s.ai_hint_en.clone()),
        };
        from_strings
            .or_else(|| s.and_then(|s| s.ai_hint_en.clone()))
            .or_else(|| self.manifest.description.clone())
            .unwrap_or_default()
    }

    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        validate_against_manifest(&self.manifest, params)
    }

    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let errs = self.validate(params);
        if !errs.is_empty() {
            return Err(ModuleError::InvalidParams(errs));
        }

        let mut effective_params = params.clone();
        let output_dir = resolve_output_dir(&self.manifest, &mut effective_params, project_dir)?;
        std::fs::create_dir_all(&output_dir)?;

        let argv = build_argv(&self.binary_path, &self.manifest, &effective_params)
            .map_err(|e| ModuleError::ToolError(e.to_string()))?;
        let after = argv[1..].to_vec();

        let _ = events_tx
            .send(RunEvent::Progress { fraction: 0.0, message: format!("Running {}", self.manifest.name) })
            .await;

        let exit_code = run_streamed(&self.binary_path, &after, events_tx.clone(), cancel).await?;

        let output_files = discover_outputs(&self.manifest, &output_dir);
        let _ = events_tx
            .send(RunEvent::Progress { fraction: 1.0, message: "Done".into() })
            .await;

        Ok(ModuleResult {
            output_files: output_files.clone(),
            summary: serde_json::json!({
                "plugin_id": self.manifest.id,
                "exit_code": exit_code,
                "argv": argv,
                "output_dir": output_dir.display().to_string(),
                "output_files": output_files.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            }),
            log: String::new(),
        })
    }
}

fn resolve_output_dir(
    m: &PluginManifest,
    params: &mut serde_json::Value,
    project_dir: &Path,
) -> Result<PathBuf, ModuleError> {
    let output_param = m
        .params
        .iter()
        .find(|p| matches!(p.r#type, ParamType::OutputDir));
    let Some(p) = output_param else {
        return Ok(project_dir.join("output"));
    };
    let obj = params.as_object_mut().ok_or_else(|| {
        ModuleError::InvalidParams(vec![ValidationError {
            field: "_".into(),
            message: "params must be an object".into(),
        }])
    })?;
    let provided = obj
        .get(&p.name)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let resolved = match provided {
        Some(s) => PathBuf::from(s),
        None => {
            let d = project_dir.join("output");
            obj.insert(p.name.clone(), serde_json::Value::String(d.display().to_string()));
            d
        }
    };
    Ok(resolved)
}

fn discover_outputs(m: &PluginManifest, output_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Some(spec) = &m.outputs else { return files };
    for pat in &spec.patterns {
        let glob_pattern = output_dir.join(pat);
        let pattern_str = match glob_pattern.to_str() {
            Some(s) => s,
            None => continue,
        };
        let Ok(it) = glob::glob(pattern_str) else { continue };
        for entry in it.flatten() {
            files.push(entry);
        }
    }
    files
}
