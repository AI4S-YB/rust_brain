use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Read,
    Run,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub risk: RiskLevel,
    /// JSON Schema draft-07 for the tool's arguments (object form).
    pub params: serde_json::Value,
}

impl ToolDef {
    /// Validate that `params` is a well-formed JSON Schema draft-07 AND that
    /// its root `type` is `"object"` — every tool receives a named-arg object.
    pub fn validate_schema(&self) -> Result<(), String> {
        let compiled = jsonschema::JSONSchema::options()
            .with_draft(jsonschema::Draft::Draft7)
            .compile(&self.params)
            .map_err(|e| format!("invalid schema for {}: {}", self.name, e))?;
        drop(compiled);
        if self.params.get("type") != Some(&serde_json::json!("object")) {
            return Err(format!(
                "tool {} schema.type must be 'object', got {:?}",
                self.name,
                self.params.get("type")
            ));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    Unknown(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("not implemented in Phase 1: {0}")]
    Unimplemented(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooldef_serializes_risk_as_lowercase_string() {
        let t = ToolDef {
            name: "x".into(),
            description: String::new(),
            risk: RiskLevel::Run,
            params: serde_json::json!({"type": "object"}),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""risk":"run""#));
    }

    #[test]
    fn validate_schema_rejects_non_object_root() {
        let t = ToolDef {
            name: "bad".into(),
            description: String::new(),
            risk: RiskLevel::Read,
            params: serde_json::json!({"type": "string"}),
        };
        assert!(t.validate_schema().is_err());
    }

    #[test]
    fn validate_schema_accepts_well_formed_object() {
        let t = ToolDef {
            name: "ok".into(),
            description: String::new(),
            risk: RiskLevel::Read,
            params: serde_json::json!({
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "required": ["x"]
            }),
        };
        assert!(t.validate_schema().is_ok());
    }
}
