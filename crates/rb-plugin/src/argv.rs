//! Build a `Vec<String>` argv from a manifest + runtime params Value.
//!
//! Pure function. No shell. The output is fed to `tokio::process::Command::args`,
//! so tokens are passed verbatim (no splitting, no quoting required).

use crate::manifest::{CliRule, ParamType, PluginManifest, ParamSpec};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ArgvError {
    #[error("required param '{0}' missing and no default")]
    MissingRequired(String),
    #[error("param '{0}' has wrong type for cli rule: {1}")]
    TypeMismatch(String, String),
    #[error("raw arg '{0}' could not be shlex-split")]
    BadRaw(String),
}

pub fn build_argv(
    binary_path: &std::path::Path,
    manifest: &PluginManifest,
    params: &Value,
) -> Result<Vec<String>, ArgvError> {
    let mut out = vec![binary_path.to_string_lossy().to_string()];
    let obj = params.as_object().cloned().unwrap_or_default();

    for p in &manifest.params {
        let v = obj.get(&p.name).cloned().or_else(|| p.default.clone());
        let v = match v {
            Some(v) => v,
            None => {
                if p.required {
                    return Err(ArgvError::MissingRequired(p.name.clone()));
                }
                continue;
            }
        };
        render_param(p, &v, &mut out)?;
    }
    Ok(out)
}

fn render_param(p: &ParamSpec, v: &Value, out: &mut Vec<String>) -> Result<(), ArgvError> {
    match &p.cli {
        CliRule::Raw { raw: false } | CliRule::Positional { positional: false } => {
            // explicit no-op rules
        }
        CliRule::Raw { raw: true } => {
            let s = v
                .as_str()
                .ok_or_else(|| ArgvError::TypeMismatch(p.name.clone(), "raw needs a string".into()))?;
            if s.is_empty() {
                return Ok(());
            }
            let parts =
                shlex::split(s).ok_or_else(|| ArgvError::BadRaw(s.to_string()))?;
            out.extend(parts);
        }
        CliRule::Positional { positional: true } => {
            extend_values(p, v, out)?;
        }
        CliRule::Flag { flag, repeat_per_value, join_with } => {
            if matches!(p.r#type, ParamType::Boolean) {
                if v.as_bool().unwrap_or(false) {
                    out.push(flag.clone());
                }
                return Ok(());
            }
            if matches!(p.r#type, ParamType::FileList) {
                let arr = v
                    .as_array()
                    .ok_or_else(|| ArgvError::TypeMismatch(p.name.clone(), "file_list needs array".into()))?;
                if arr.is_empty() {
                    return Ok(());
                }
                if *repeat_per_value {
                    for item in arr {
                        let s = item.as_str().ok_or_else(|| {
                            ArgvError::TypeMismatch(p.name.clone(), "file_list items must be strings".into())
                        })?;
                        out.push(flag.clone());
                        out.push(s.to_string());
                    }
                } else if let Some(sep) = join_with {
                    let mut parts: Vec<String> = Vec::with_capacity(arr.len());
                    for item in arr {
                        let s = item.as_str().ok_or_else(|| {
                            ArgvError::TypeMismatch(p.name.clone(), "file_list items must be strings".into())
                        })?;
                        parts.push(s.to_string());
                    }
                    out.push(flag.clone());
                    out.push(parts.join(sep));
                } else {
                    out.push(flag.clone());
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            out.push(s.to_string());
                        }
                    }
                }
                return Ok(());
            }
            // scalar value
            out.push(flag.clone());
            out.push(value_to_string(v));
        }
    }
    Ok(())
}

fn extend_values(p: &ParamSpec, v: &Value, out: &mut Vec<String>) -> Result<(), ArgvError> {
    if matches!(p.r#type, ParamType::FileList) {
        let arr = v
            .as_array()
            .ok_or_else(|| ArgvError::TypeMismatch(p.name.clone(), "file_list needs array".into()))?;
        for item in arr {
            if let Some(s) = item.as_str() {
                out.push(s.to_string());
            }
        }
    } else {
        out.push(value_to_string(v));
    }
    Ok(())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use serde_json::json;
    use std::path::Path;

    fn manifest(toml_str: &str) -> PluginManifest {
        toml::from_str(toml_str).expect("parse")
    }

    #[test]
    fn flag_with_scalar_value() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "threads"
            type = "integer"
            cli = { flag = "--threads" }
            "#,
        );
        let argv = build_argv(Path::new("/usr/bin/x"), &m, &json!({"threads": 4})).unwrap();
        assert_eq!(argv, vec!["/usr/bin/x", "--threads", "4"]);
    }

    #[test]
    fn boolean_flag_present_only_when_true() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "quiet"
            type = "boolean"
            cli = { flag = "--quiet" }
            "#,
        );
        let on = build_argv(Path::new("/x"), &m, &json!({"quiet": true})).unwrap();
        let off = build_argv(Path::new("/x"), &m, &json!({"quiet": false})).unwrap();
        assert_eq!(on, vec!["/x", "--quiet"]);
        assert_eq!(off, vec!["/x"]);
    }

    #[test]
    fn file_list_repeat_per_value() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "inputs"
            type = "file_list"
            cli = { flag = "-i", repeat_per_value = true }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"inputs": ["a.fq", "b.fq"]}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "-i", "a.fq", "-i", "b.fq"]);
    }

    #[test]
    fn file_list_joined_with_comma() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "inputs"
            type = "file_list"
            cli = { flag = "-I", join_with = "," }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"inputs": ["a", "b", "c"]}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "-I", "a,b,c"]);
    }

    #[test]
    fn positional_file_list() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "inputs"
            type = "file_list"
            cli = { positional = true }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"inputs": ["a", "b"]}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "a", "b"]);
    }

    #[test]
    fn raw_extra_args_split_with_shlex() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "extra"
            type = "string"
            cli = { raw = true }
            "#,
        );
        let argv = build_argv(
            Path::new("/x"),
            &m,
            &json!({"extra": "--foo bar --baz \"two words\""}),
        )
        .unwrap();
        assert_eq!(argv, vec!["/x", "--foo", "bar", "--baz", "two words"]);
    }

    #[test]
    fn raw_empty_string_is_ignored() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "extra"
            type = "string"
            cli = { raw = true }
            "#,
        );
        let argv = build_argv(Path::new("/x"), &m, &json!({"extra": ""})).unwrap();
        assert_eq!(argv, vec!["/x"]);
    }

    #[test]
    fn missing_required_errors() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "inputs"
            type = "file_list"
            required = true
            cli = { flag = "-i", repeat_per_value = true }
            "#,
        );
        let err = build_argv(Path::new("/x"), &m, &json!({})).unwrap_err();
        assert!(matches!(err, ArgvError::MissingRequired(ref n) if n == "inputs"));
    }

    #[test]
    fn default_used_when_param_missing() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "threads"
            type = "integer"
            default = 8
            cli = { flag = "--threads" }
            "#,
        );
        let argv = build_argv(Path::new("/x"), &m, &json!({})).unwrap();
        assert_eq!(argv, vec!["/x", "--threads", "8"]);
    }

    #[test]
    fn order_follows_manifest_declaration() {
        let m = manifest(
            r#"
            id = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "t"
            type = "integer"
            default = 1
            cli = { flag = "-t" }
            [[params]]
            name = "o"
            type = "output_dir"
            default = "out"
            cli = { flag = "-o" }
            [[params]]
            name = "i"
            type = "file_list"
            default = ["a"]
            cli = { flag = "-i", repeat_per_value = true }
            "#,
        );
        let argv = build_argv(Path::new("/x"), &m, &json!({})).unwrap();
        assert_eq!(argv, vec!["/x", "-t", "1", "-o", "out", "-i", "a"]);
    }

    #[test]
    fn bad_raw_unterminated_quote_errors() {
        let m = manifest(
            r#"
            id   = "x"
            name = "X"
            [binary]
            id = "x"
            [[params]]
            name = "extra"
            type = "string"
            cli  = { raw = true }
            "#,
        );
        let err = build_argv(Path::new("/x"), &m, &json!({"extra": "--foo \"unterminated"}))
            .unwrap_err();
        assert!(matches!(err, ArgvError::BadRaw(_)));
    }
}
