//! Manifest + runtime parameter validation.
//!
//! Validation is a pure function over the manifest data — no I/O, no
//! filesystem access. Returns a list of issues so callers can display all
//! problems at once.

use crate::manifest::{CliRule, ParamSpec, ParamType, PluginManifest};

pub const SUPPORTED_MANIFEST_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestIssueLevel {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct ManifestIssue {
    pub field: String,
    pub message: String,
    pub level: ManifestIssueLevel,
}

impl ManifestIssue {
    fn err(field: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { field: field.into(), message: msg.into(), level: ManifestIssueLevel::Error }
    }
}

pub fn validate_manifest(m: &PluginManifest) -> Vec<ManifestIssue> {
    let mut out = Vec::new();

    if m.id.trim().is_empty() {
        out.push(ManifestIssue::err("id", "id must be non-empty"));
    }
    if m.name.trim().is_empty() {
        out.push(ManifestIssue::err("name", "name must be non-empty"));
    }
    if m.binary.id.trim().is_empty() {
        out.push(ManifestIssue::err("binary.id", "binary.id must be non-empty"));
    }
    if let Some(v) = m.version.as_deref() {
        if v != SUPPORTED_MANIFEST_VERSION {
            out.push(ManifestIssue::err(
                "version",
                format!("unsupported manifest version '{}', expected '{}'", v, SUPPORTED_MANIFEST_VERSION),
            ));
        }
    }

    let mut seen = std::collections::HashSet::new();
    for (i, p) in m.params.iter().enumerate() {
        let prefix = format!("params[{}]", i);
        if !seen.insert(p.name.clone()) {
            out.push(ManifestIssue::err(format!("{prefix}.name"), format!("duplicate param name '{}'", p.name)));
        }
        if p.required && p.default.is_some() {
            out.push(ManifestIssue::err(
                format!("{prefix}.required"),
                "required and default are mutually exclusive — pick one",
            ));
        }
        if matches!(p.r#type, ParamType::Enum) && p.values.is_empty() {
            out.push(ManifestIssue::err(
                format!("{prefix}.values"),
                "enum params must declare a non-empty `values` list",
            ));
        }
        validate_cli_rule(&p.cli, &prefix, &mut out, p);
    }

    out
}

fn validate_cli_rule(
    rule: &CliRule,
    prefix: &str,
    out: &mut Vec<ManifestIssue>,
    p: &ParamSpec,
) {
    match rule {
        CliRule::Flag { flag, repeat_per_value, join_with } => {
            if flag.trim().is_empty() {
                out.push(ManifestIssue::err(format!("{prefix}.cli.flag"), "flag must be non-empty"));
            }
            if *repeat_per_value && join_with.is_some() {
                out.push(ManifestIssue::err(
                    format!("{prefix}.cli"),
                    "repeat_per_value and join_with are mutually exclusive",
                ));
            }
            if (*repeat_per_value || join_with.is_some()) && !matches!(p.r#type, ParamType::FileList) {
                out.push(ManifestIssue::err(
                    format!("{prefix}.cli"),
                    "repeat_per_value / join_with apply only to file_list params",
                ));
            }
        }
        CliRule::Positional { .. } | CliRule::Raw { .. } => {}
    }
}

/// Validate a runtime params Value against a manifest. Returns
/// `rb_core::module::ValidationError` so it slots straight into
/// `Module::validate()`.
pub fn validate_against_manifest(
    m: &PluginManifest,
    params: &serde_json::Value,
) -> Vec<rb_core::module::ValidationError> {
    use rb_core::module::ValidationError;
    let mut errs = Vec::new();
    let obj = match params.as_object() {
        Some(o) => o,
        None => {
            errs.push(ValidationError {
                field: "_".into(),
                message: "params must be a JSON object".into(),
            });
            return errs;
        }
    };

    for p in &m.params {
        let v = obj.get(&p.name);
        if v.is_none() && p.required && p.default.is_none() {
            errs.push(ValidationError {
                field: p.name.clone(),
                message: format!("'{}' is required", p.name),
            });
            continue;
        }
        let Some(v) = v else { continue };
        type_check(p, v, &mut errs);
    }
    errs
}

fn type_check(p: &ParamSpec, v: &serde_json::Value, errs: &mut Vec<rb_core::module::ValidationError>) {
    use rb_core::module::ValidationError;
    let mismatch = |msg: String| ValidationError { field: p.name.clone(), message: msg };
    match p.r#type {
        ParamType::String | ParamType::OutputDir => {
            if !v.is_string() {
                errs.push(mismatch(format!("'{}' must be a string", p.name)));
            }
        }
        ParamType::Integer => {
            if !v.is_i64() && !v.is_u64() {
                errs.push(mismatch(format!("'{}' must be an integer", p.name)));
            } else if let Some(n) = v.as_f64() {
                if let Some(min) = p.minimum {
                    if n < min {
                        errs.push(mismatch(format!("'{}' must be >= {}", p.name, min)));
                    }
                }
                if let Some(max) = p.maximum {
                    if n > max {
                        errs.push(mismatch(format!("'{}' must be <= {}", p.name, max)));
                    }
                }
            }
        }
        ParamType::Boolean => {
            if !v.is_boolean() {
                errs.push(mismatch(format!("'{}' must be a boolean", p.name)));
            }
        }
        ParamType::File | ParamType::Directory => {
            match v.as_str() {
                None => errs.push(mismatch(format!("'{}' must be a path string", p.name))),
                Some(s) => {
                    let path = std::path::Path::new(s);
                    let ok = match p.r#type {
                        ParamType::File => path.is_file(),
                        ParamType::Directory => path.is_dir(),
                        _ => true,
                    };
                    if !ok {
                        errs.push(mismatch(format!("'{}': path does not exist or wrong kind: {}", p.name, s)));
                    }
                }
            }
        }
        ParamType::FileList => match v.as_array() {
            None => errs.push(mismatch(format!("'{}' must be an array of paths", p.name))),
            Some(arr) => {
                if p.required && arr.is_empty() {
                    errs.push(mismatch(format!("'{}' must be non-empty", p.name)));
                }
                for (i, item) in arr.iter().enumerate() {
                    if !item.is_string() {
                        errs.push(mismatch(format!("'{}'[{}] must be a string", p.name, i)));
                    }
                }
            }
        },
        ParamType::Enum => match v.as_str() {
            None => errs.push(mismatch(format!("'{}' must be a string", p.name))),
            Some(s) => {
                if !p.values.iter().any(|allowed| allowed == s) {
                    errs.push(mismatch(format!(
                        "'{}' must be one of: {}",
                        p.name,
                        p.values.join(", ")
                    )));
                }
            }
        },
    }
}
