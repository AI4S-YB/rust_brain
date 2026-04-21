//! Scan plugin directories, parse + validate, dedupe by id (user wins).

use crate::manifest::PluginManifest;
use crate::validate::{validate_manifest, ManifestIssueLevel};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource {
    Bundled,
    User,
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub source: PluginSource,
    pub origin_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PluginLoadError {
    pub source_label: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    pub by_id: HashMap<String, LoadedPlugin>,
    pub errors: Vec<PluginLoadError>,
}

/// Load plugins from an embedded bundled dir + an optional user dir on disk.
pub fn load_plugins(
    bundled: &include_dir::Dir<'_>,
    user_dir: Option<&Path>,
) -> PluginRegistry {
    let mut reg = PluginRegistry::default();

    for f in bundled.files() {
        if f.path().extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        let label = format!("bundled:{}", f.path().display());
        let text = match std::str::from_utf8(f.contents()) {
            Ok(t) => t,
            Err(_) => {
                reg.errors.push(PluginLoadError {
                    source_label: label,
                    message: "non-UTF8 manifest".into(),
                });
                continue;
            }
        };
        match parse_one(text) {
            Ok(m) => {
                reg.by_id.insert(
                    m.id.clone(),
                    LoadedPlugin {
                        manifest: m,
                        source: PluginSource::Bundled,
                        origin_path: None,
                    },
                );
            }
            Err(e) => reg.errors.push(PluginLoadError { source_label: label, message: e }),
        }
    }

    if let Some(dir) = user_dir {
        if dir.exists() {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(e) => {
                    reg.errors.push(PluginLoadError {
                        source_label: format!("user:{}", dir.display()),
                        message: format!("read_dir failed: {e}"),
                    });
                    return reg;
                }
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                    continue;
                }
                let label = format!("user:{}", path.display());
                let text = match std::fs::read_to_string(&path) {
                    Ok(t) => t,
                    Err(e) => {
                        reg.errors.push(PluginLoadError {
                            source_label: label,
                            message: e.to_string(),
                        });
                        continue;
                    }
                };
                match parse_one(&text) {
                    Ok(m) => {
                        reg.by_id.insert(
                            m.id.clone(),
                            LoadedPlugin {
                                manifest: m,
                                source: PluginSource::User,
                                origin_path: Some(path),
                            },
                        );
                    }
                    Err(e) => reg.errors.push(PluginLoadError { source_label: label, message: e }),
                }
            }
        }
    }
    reg
}

fn parse_one(text: &str) -> Result<PluginManifest, String> {
    let m: PluginManifest =
        toml::from_str(text).map_err(|e| format!("toml parse error: {e}"))?;
    let issues = validate_manifest(&m);
    let errors: Vec<_> = issues
        .into_iter()
        .filter(|i| i.level == ManifestIssueLevel::Error)
        .collect();
    if !errors.is_empty() {
        let joined = errors
            .iter()
            .map(|i| format!("{}: {}", i.field, i.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(joined);
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    // include_dir 0.7.x: Dir::new(path, entries) — two args, not three.
    static EMPTY_BUNDLED: include_dir::Dir<'_> = include_dir::Dir::new("empty", &[]);

    #[test]
    fn user_dir_loads_valid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "rustqc.toml",
            include_str!("../tests/data/rustqc.toml"),
        );
        let reg = load_plugins(&EMPTY_BUNDLED, Some(tmp.path()));
        assert_eq!(reg.by_id.len(), 1);
        assert!(reg.by_id.contains_key("rustqc"));
        assert_eq!(reg.by_id["rustqc"].source, PluginSource::User);
        assert!(reg.errors.is_empty());
    }

    #[test]
    fn user_dir_with_invalid_toml_records_error() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "broken.toml", "this is not = valid toml [[[[");
        let reg = load_plugins(&EMPTY_BUNDLED, Some(tmp.path()));
        assert!(reg.by_id.is_empty());
        assert_eq!(reg.errors.len(), 1);
        assert!(reg.errors[0].source_label.ends_with("broken.toml"));
    }

    #[test]
    fn missing_user_dir_is_ok() {
        let reg = load_plugins(&EMPTY_BUNDLED, Some(Path::new("/nope/does/not/exist")));
        assert!(reg.by_id.is_empty());
        assert!(reg.errors.is_empty());
    }
}
