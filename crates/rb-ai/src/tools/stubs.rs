use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::schema::{RiskLevel, ToolDef, ToolError};
use super::{ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry};

pub fn register_all(registry: &mut ToolRegistry) {
    registry.register(entry(
        "read_results_table",
        "Read a results table (TSV/CSV/Parquet) with optional projection/filter. \
         Not implemented in Phase 1 — will land in the analysis-agent phase.",
        RiskLevel::Read,
        json!({
            "type": "object",
            "properties": {
                "run_id":  { "type": "string" },
                "path":    { "type": "string" },
                "columns": { "type": "array", "items": { "type": "string" } },
                "filter":  {
                    "type": "string",
                    "description": "polars SQL-lite filter expression"
                },
                "limit":   { "type": "integer", "minimum": 1, "maximum": 10000 }
            },
            "required": ["run_id"],
            "additionalProperties": false
        }),
    ));
    registry.register(entry(
        "generate_plot",
        "Produce an ECharts JSON spec for a custom visualization. \
         Not implemented in Phase 1.",
        RiskLevel::Read,
        json!({
            "type": "object",
            "properties": {
                "source_run_id": { "type": "string" },
                "kind":          {
                    "type": "string",
                    "enum": ["volcano", "pca", "heatmap"]
                }
            },
            "required": ["source_run_id", "kind"],
            "additionalProperties": false
        }),
    ));
}

fn entry(name: &str, desc: &str, risk: RiskLevel, params: Value) -> ToolEntry {
    ToolEntry {
        def: ToolDef {
            name: name.into(),
            description: desc.into(),
            risk,
            params,
        },
        executor: Arc::new(UnimplementedStub { name: name.into() }),
    }
}

pub struct UnimplementedStub {
    pub name: String,
}

#[async_trait]
impl ToolExecutor for UnimplementedStub {
    async fn execute(&self, _args: &Value, _ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Unimplemented(format!(
            "{} is reserved for a future release; fall back to run_* tools and describe findings in text.",
            self.name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolRegistry;

    #[test]
    fn register_all_adds_reserved_stubs_with_valid_schema() {
        let mut r = ToolRegistry::new();
        register_all(&mut r);
        for n in ["read_results_table", "generate_plot"] {
            let t = r.get(n).unwrap_or_else(|| panic!("missing {n}"));
            t.def.validate_schema().unwrap();
        }
    }
}
