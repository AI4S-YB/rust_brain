//! Derive a JSON Schema (draft-07-ish, matching what rb-ai expects) from a
//! manifest's params. Used by:
//!   - `Module::params_schema()` so plugins surface in the AI tool registry
//!   - `validate_params` Tauri command so the frontend gets the same shape
//!     of errors as for first-party modules

use crate::manifest::{ParamType, PluginManifest};
use serde_json::{json, Map, Value};

pub fn derive_json_schema(m: &PluginManifest) -> Value {
    let mut props = Map::new();
    let mut required = Vec::new();
    for p in &m.params {
        let mut entry = Map::new();
        match p.r#type {
            ParamType::String | ParamType::OutputDir | ParamType::File | ParamType::Directory => {
                entry.insert("type".into(), json!("string"));
            }
            ParamType::Integer => {
                entry.insert("type".into(), json!("integer"));
                if let Some(min) = p.minimum {
                    entry.insert("minimum".into(), json!(min));
                }
                if let Some(max) = p.maximum {
                    entry.insert("maximum".into(), json!(max));
                }
            }
            ParamType::Boolean => {
                entry.insert("type".into(), json!("boolean"));
            }
            ParamType::FileList => {
                entry.insert("type".into(), json!("array"));
                entry.insert("items".into(), json!({"type": "string"}));
            }
            ParamType::Enum => {
                entry.insert("type".into(), json!("string"));
                entry.insert("enum".into(), json!(p.values));
            }
        }
        if let Some(desc) = p.help_en.clone().or_else(|| p.label_en.clone()) {
            entry.insert("description".into(), json!(desc));
        }
        if let Some(d) = &p.default {
            entry.insert("default".into(), d.clone());
        }
        props.insert(p.name.clone(), Value::Object(entry));
        if p.required {
            required.push(p.name.clone());
        }
    }
    json!({
        "type": "object",
        "properties": props,
        "required": required,
        "additionalProperties": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;

    fn parse(s: &str) -> PluginManifest {
        toml::from_str(s).expect("parse")
    }

    #[test]
    fn derives_object_schema_with_required_list() {
        let m = parse(
            r#"
            id   = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "inputs"
            type = "file_list"
            required = true
            cli  = { flag = "-i", repeat_per_value = true }
            [[params]]
            name = "threads"
            type = "integer"
            default = 4
            minimum = 1
            maximum = 32
            cli  = { flag = "-t" }
            [[params]]
            name = "fmt"
            type = "enum"
            values = ["a","b"]
            default = "a"
            cli  = { flag = "--fmt" }
            "#,
        );
        let s = derive_json_schema(&m);
        assert_eq!(s["type"], "object");
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["required"], json!(["inputs"]));
        assert_eq!(s["properties"]["inputs"]["type"], "array");
        assert_eq!(s["properties"]["inputs"]["items"]["type"], "string");
        assert_eq!(s["properties"]["threads"]["type"], "integer");
        assert_eq!(s["properties"]["threads"]["minimum"], 1.0);
        assert_eq!(s["properties"]["threads"]["maximum"], 32.0);
        assert_eq!(s["properties"]["threads"]["default"], 4);
        assert_eq!(s["properties"]["fmt"]["type"], "string");
        assert_eq!(s["properties"]["fmt"]["enum"], json!(["a", "b"]));
    }

    #[test]
    fn rustqc_fixture_derives_valid_schema() {
        let m: PluginManifest = toml::from_str(include_str!("../tests/data/rustqc.toml")).unwrap();
        let s = derive_json_schema(&m);
        assert_eq!(s["required"], json!(["input_files"]));
        assert!(s["properties"].as_object().unwrap().contains_key("nogroup"));
    }
}
