use std::sync::Arc;

use async_trait::async_trait;
use rb_core::module::Module;
use serde_json::Value;

use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};

/// Register a `run_{module.id}` Run-risk tool for every module whose
/// `params_schema` is `Some(_)`. Modules returning `None` are skipped so
/// half-specified modules can't accidentally be invoked by the LLM.
pub fn register_for_modules(registry: &mut ToolRegistry, modules: &[Arc<dyn Module>], lang: &str) {
    for m in modules {
        let Some(schema) = m.params_schema() else {
            continue;
        };
        let name = format!("run_{}", m.id());
        let hint = m.ai_hint(lang);
        let description = if hint.is_empty() {
            format!("Run the {} module.", m.name())
        } else {
            hint
        };
        registry.register(ToolEntry {
            def: ToolDef {
                name,
                description,
                risk: RiskLevel::Run,
                params: schema,
            },
            executor: Arc::new(ModuleRunExec { module: m.clone() }),
        });
    }
}

pub struct ModuleRunExec {
    pub module: Arc<dyn Module>,
}

#[async_trait]
impl ToolExecutor for ModuleRunExec {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        // Validate before spawning so schema-level errors surface as
        // InvalidArgs (recoverable at the LLM layer) rather than Execution.
        let errs = self.module.validate(args);
        if !errs.is_empty() {
            return Err(ToolError::InvalidArgs(
                errs.iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let run_id = ctx
            .runner
            .spawn(self.module.clone(), args.clone(), Vec::new(), Vec::new())
            .await
            .map_err(ToolError::Execution)?;
        Ok(ToolOutput::Value(serde_json::json!({
            "run_id": run_id,
            "status": "started",
            "module_id": self.module.id(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rb_core::cancel::CancellationToken;
    use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
    use rb_core::run_event::RunEvent;
    use serde_json::json;
    use std::path::Path;
    use tokio::sync::mpsc;

    struct OkModule;
    #[async_trait]
    impl Module for OkModule {
        fn id(&self) -> &str {
            "ok"
        }
        fn name(&self) -> &str {
            "OK"
        }
        fn validate(&self, _p: &Value) -> Vec<ValidationError> {
            vec![]
        }
        fn params_schema(&self) -> Option<Value> {
            Some(json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true
            }))
        }
        fn ai_hint(&self, _l: &str) -> String {
            "ok module".into()
        }
        async fn run(
            &self,
            _: &Value,
            _: &Path,
            tx: mpsc::Sender<RunEvent>,
            _: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            let _ = tx
                .send(RunEvent::Progress {
                    fraction: 1.0,
                    message: "done".into(),
                })
                .await;
            Ok(ModuleResult {
                output_files: vec![],
                summary: json!({}),
                log: String::new(),
            })
        }
    }

    struct SilentModule;
    #[async_trait]
    impl Module for SilentModule {
        fn id(&self) -> &str {
            "silent"
        }
        fn name(&self) -> &str {
            "Silent"
        }
        fn validate(&self, _p: &Value) -> Vec<ValidationError> {
            vec![]
        }
        // params_schema defaults to None — should be skipped.
        async fn run(
            &self,
            _: &Value,
            _: &Path,
            _: mpsc::Sender<RunEvent>,
            _: CancellationToken,
        ) -> Result<ModuleResult, ModuleError> {
            Ok(ModuleResult {
                output_files: vec![],
                summary: json!({}),
                log: String::new(),
            })
        }
    }

    #[test]
    fn module_without_schema_is_skipped() {
        let mut reg = ToolRegistry::new();
        let mods: Vec<Arc<dyn Module>> = vec![Arc::new(OkModule), Arc::new(SilentModule)];
        register_for_modules(&mut reg, &mods, "en");
        assert!(reg.get("run_ok").is_some());
        assert!(
            reg.get("run_silent").is_none(),
            "modules without a schema must not be registered"
        );
    }

    #[test]
    fn derived_tool_uses_ai_hint_when_present() {
        let mut reg = ToolRegistry::new();
        let mods: Vec<Arc<dyn Module>> = vec![Arc::new(OkModule)];
        register_for_modules(&mut reg, &mods, "en");
        let entry = reg.get("run_ok").unwrap();
        assert_eq!(entry.def.description, "ok module");
        assert_eq!(entry.def.risk, RiskLevel::Run);
    }
}
