pub mod builtin;
pub mod module_derived;
pub mod schema;
pub mod stubs;

pub use schema::{RiskLevel, ToolDef, ToolError};

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use rb_core::project::Project;
use rb_core::runner::Runner;

/// Context handed to tool executors. Gives them project-level access
/// without leaking `ModuleRegistry` internals.
pub struct ToolContext<'a> {
    pub project: &'a Arc<tokio::sync::Mutex<Project>>,
    pub runner: &'a Arc<Runner>,
    pub binary_resolver: &'a Arc<tokio::sync::Mutex<rb_core::binary::BinaryResolver>>,
}

/// Outcome of executing a tool. Single-variant enum reserves room to add
/// streaming / partial results later without touching callers.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum ToolOutput {
    Value(Value),
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, args: &Value, ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError>;
}

pub struct ToolEntry {
    pub def: ToolDef,
    pub executor: Arc<dyn ToolExecutor>,
}

#[derive(Default)]
pub struct ToolRegistry {
    entries: HashMap<String, ToolEntry>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn register(&mut self, entry: ToolEntry) {
        self.entries.insert(entry.def.name.clone(), entry);
    }

    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.entries.get(name)
    }

    /// Deterministic, sorted list of `ToolDef`s for serialization into provider
    /// requests.
    pub fn all_for_ai(&self) -> Vec<ToolDef> {
        let mut v: Vec<_> = self.entries.values().map(|e| e.def.clone()).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoExec;

    #[async_trait]
    impl ToolExecutor for EchoExec {
        async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::Value(args.clone()))
        }
    }

    #[test]
    fn registry_returns_tools_sorted_and_findable_by_name() {
        let mut r = ToolRegistry::new();
        r.register(ToolEntry {
            def: ToolDef {
                name: "b_tool".into(),
                description: "b".into(),
                risk: RiskLevel::Read,
                params: serde_json::json!({"type":"object"}),
            },
            executor: Arc::new(EchoExec),
        });
        r.register(ToolEntry {
            def: ToolDef {
                name: "a_tool".into(),
                description: "a".into(),
                risk: RiskLevel::Read,
                params: serde_json::json!({"type":"object"}),
            },
            executor: Arc::new(EchoExec),
        });
        let all = r.all_for_ai();
        assert_eq!(all[0].name, "a_tool");
        assert_eq!(all[1].name, "b_tool");
        assert!(r.get("a_tool").is_some());
        assert!(r.get("missing").is_none());
    }
}
