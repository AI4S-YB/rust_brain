//! TOML manifest data types.
//!
//! Hand-written serde structs (no schema crates) so the surface stays small
//! and validation messages cite the exact toml field names. Validation
//! rules live in `validate.rs`; this file only describes shape.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub strings: Option<Strings>,
    pub binary: BinarySpec,
    #[serde(default)]
    pub params: Vec<ParamSpec>,
    #[serde(default)]
    pub outputs: Option<OutputSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Strings {
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub name_zh: Option<String>,
    #[serde(default)]
    pub description_en: Option<String>,
    #[serde(default)]
    pub description_zh: Option<String>,
    #[serde(default)]
    pub ai_hint_en: Option<String>,
    #[serde(default)]
    pub ai_hint_zh: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinarySpec {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub install_hint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub r#type: ParamType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub label_en: Option<String>,
    #[serde(default)]
    pub label_zh: Option<String>,
    #[serde(default)]
    pub help_en: Option<String>,
    #[serde(default)]
    pub help_zh: Option<String>,
    #[serde(default)]
    pub ui: Option<String>,
    #[serde(default)]
    pub minimum: Option<f64>,
    #[serde(default)]
    pub maximum: Option<f64>,
    #[serde(default)]
    pub values: Vec<String>,
    pub cli: CliRule,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Integer,
    Boolean,
    File,
    FileList,
    Directory,
    Enum,
    OutputDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum CliRule {
    Positional {
        positional: bool,
    },
    Raw {
        raw: bool,
    },
    Flag {
        flag: String,
        #[serde(default)]
        repeat_per_value: bool,
        #[serde(default)]
        join_with: Option<String>,
    },
}

impl CliRule {
    pub fn is_positional(&self) -> bool {
        matches!(self, CliRule::Positional { positional: true })
    }
    pub fn is_raw(&self) -> bool {
        matches!(self, CliRule::Raw { raw: true })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OutputSpec {
    #[serde(default)]
    pub patterns: Vec<String>,
}
