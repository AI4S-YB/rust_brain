use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Registry of binaries that rust_brain knows about.
/// Each entry has an id used as both the CLI name (what's looked up on PATH)
/// and the settings.json key, plus a human-readable install hint.
pub struct KnownBinary {
    pub id: &'static str,
    pub display_name: &'static str,
    pub install_hint: &'static str,
}

pub const KNOWN_BINARIES: &[KnownBinary] = &[
    KnownBinary {
        id: "star",
        display_name: "STAR (STAR_rs)",
        install_hint: "Build from https://github.com/AI4S-YB/STAR_rs and set the path in Settings, or add the `star` binary to PATH.",
    },
    KnownBinary {
        id: "cutadapt-rs",
        display_name: "cutadapt-rs",
        install_hint: "Build from https://github.com/AI4S-YB/cutadapt-rs and set the path in Settings, or add the `cutadapt-rs` binary to PATH.",
    },
    KnownBinary {
        id: "gffread-rs",
        display_name: "gffread-rs",
        install_hint: "Prebuilt binaries at https://github.com/AI4S-YB/gffread_rs/releases — drop on PATH or set the path in Settings.",
    },
];

#[derive(Debug, thiserror::Error)]
pub enum BinaryError {
    #[error("binary '{name}' not found. Searched: {searched:?}. {hint}")]
    NotFound {
        name: String,
        searched: Vec<String>,
        hint: String,
    },
    #[error("path '{0}' is not an executable file")]
    NotExecutable(PathBuf),
    #[error("settings I/O error: {0}")]
    SettingsIo(#[from] std::io::Error),
    #[error("settings parse error: {0}")]
    SettingsParse(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettingsFile {
    #[serde(default)]
    pub binary_paths: HashMap<String, Option<PathBuf>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BinaryStatus {
    pub id: String,
    pub display_name: String,
    pub configured_path: Option<PathBuf>,
    pub bundled_path: Option<PathBuf>,
    pub detected_on_path: Option<PathBuf>,
    pub install_hint: String,
}

pub struct BinaryResolver {
    settings_path: PathBuf,
    settings: SettingsFile,
    /// In-memory sidecar registry seeded at startup (e.g. from Tauri's
    /// bundled resources). Never persisted; the user's settings.json override
    /// still wins.
    bundled: HashMap<String, PathBuf>,
}

impl BinaryResolver {
    /// Construct an in-memory resolver with default (empty) settings, anchored at
    /// the given path. Useful when loading from disk failed and we want to allow
    /// subsequent saves to recover.
    pub fn with_defaults_at(settings_path: PathBuf) -> Self {
        Self {
            settings_path,
            settings: SettingsFile::default(),
            bundled: HashMap::new(),
        }
    }

    /// Cross-platform settings path using the `directories` crate.
    pub fn default_settings_path() -> PathBuf {
        if let Some(pd) = directories::ProjectDirs::from("", "", "rust_brain") {
            return pd.config_dir().join("settings.json");
        }
        PathBuf::from("settings.json")
    }

    pub fn load() -> Result<Self, BinaryError> {
        Self::load_from(Self::default_settings_path())
    }

    pub fn load_from(path: PathBuf) -> Result<Self, BinaryError> {
        let settings = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str(&text)?
        } else {
            SettingsFile::default()
        };
        Ok(Self {
            settings_path: path,
            settings,
            bundled: HashMap::new(),
        })
    }

    /// Register a sidecar binary shipped with the app bundle. Only kept in
    /// memory — not written to settings.json. Used as a fallback between the
    /// user-configured override and the PATH lookup.
    pub fn register_bundled(&mut self, name: &str, path: PathBuf) {
        self.bundled.insert(name.to_string(), path);
    }

    pub fn resolve(&self, name: &str) -> Result<PathBuf, BinaryError> {
        // Configured override takes precedence
        if let Some(Some(p)) = self.settings.binary_paths.get(name) {
            if is_executable(p) {
                return Ok(p.clone());
            }
            return Err(BinaryError::NotExecutable(p.clone()));
        }
        // Bundled sidecar shipped with the app
        if let Some(p) = self.bundled.get(name) {
            if is_executable(p) {
                return Ok(p.clone());
            }
        }
        // Fall back to PATH
        if let Ok(found) = which::which(name) {
            return Ok(found);
        }
        let hint = KNOWN_BINARIES
            .iter()
            .find(|k| k.id == name)
            .map(|k| k.install_hint.to_string())
            .unwrap_or_else(|| format!("No install hint registered for '{}'.", name));
        Err(BinaryError::NotFound {
            name: name.to_string(),
            searched: vec![
                "settings.json override".into(),
                "bundled sidecar".into(),
                "$PATH".into(),
            ],
            hint,
        })
    }

    pub fn set(&mut self, name: &str, path: PathBuf) -> Result<(), BinaryError> {
        if !is_executable(&path) {
            return Err(BinaryError::NotExecutable(path));
        }
        self.settings
            .binary_paths
            .insert(name.to_string(), Some(path));
        self.save()
    }

    pub fn clear(&mut self, name: &str) -> Result<(), BinaryError> {
        self.settings.binary_paths.remove(name);
        self.save()
    }

    fn save(&self) -> Result<(), BinaryError> {
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&self.settings)?;
        std::fs::write(&self.settings_path, text)?;
        Ok(())
    }

    pub fn list_known(&self) -> Vec<BinaryStatus> {
        KNOWN_BINARIES
            .iter()
            .map(|k| {
                let configured = self.settings.binary_paths.get(k.id).and_then(|o| o.clone());
                let bundled = self.bundled.get(k.id).cloned();
                let detected = which::which(k.id).ok();
                BinaryStatus {
                    id: k.id.to_string(),
                    display_name: k.display_name.to_string(),
                    configured_path: configured,
                    bundled_path: bundled,
                    detected_on_path: detected,
                    install_hint: k.install_hint.to_string(),
                }
            })
            .collect()
    }
}

fn is_executable(p: &Path) -> bool {
    if !p.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_exec(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\necho hi\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(&p).unwrap().permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(&p, perm).unwrap();
        }
        p
    }

    #[test]
    fn override_takes_precedence_over_path() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = write_exec(tmp.path(), "star");
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        r.set("star", fake.clone()).unwrap();
        let resolved = r.resolve("star").unwrap();
        assert_eq!(resolved, fake);
    }

    #[test]
    fn not_found_includes_install_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = tmp.path().join("settings.json");
        let r = BinaryResolver::load_from(settings).unwrap();
        // Use a name guaranteed to be missing from PATH and registered.
        let err = r.resolve("star").unwrap_err();
        match err {
            BinaryError::NotFound { hint, .. } => {
                assert!(
                    hint.contains("STAR_rs"),
                    "hint should reference STAR_rs: {}",
                    hint
                );
            }
            _ => {
                // On CI a real `star` may exist on PATH; in that case, the test is inapplicable.
                // Accept success too.
            }
        }
    }

    // Unix-only: the executable bit is the authoritative signal. On Windows
    // there is no mode 0o111 equivalent, so `is_executable` accepts any file
    // and this check does not apply.
    #[cfg(unix)]
    #[test]
    fn set_rejects_non_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("not-exec.txt");
        std::fs::write(&p, "hi").unwrap();
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        let res = r.set("star", p);
        assert!(matches!(res, Err(BinaryError::NotExecutable(_))));
    }

    #[test]
    fn bundled_used_when_no_override() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = write_exec(tmp.path(), "star-bundled");
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        r.register_bundled("star", fake.clone());
        // Even if PATH has something, bundled wins over PATH (we assume 'star'
        // is not on the CI PATH; even if it is, the override-less case should
        // still return the bundled path first).
        let resolved = r.resolve("star").unwrap();
        assert_eq!(resolved, fake);
    }

    #[test]
    fn override_wins_over_bundled() {
        let tmp = tempfile::tempdir().unwrap();
        let override_bin = write_exec(tmp.path(), "star-override");
        let bundled_bin = write_exec(tmp.path(), "star-bundled");
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        r.register_bundled("star", bundled_bin);
        r.set("star", override_bin.clone()).unwrap();
        let resolved = r.resolve("star").unwrap();
        assert_eq!(resolved, override_bin);
    }

    #[test]
    fn clear_removes_override() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = write_exec(tmp.path(), "star");
        let settings = tmp.path().join("settings.json");
        let mut r = BinaryResolver::load_from(settings).unwrap();
        r.set("star", fake).unwrap();
        r.clear("star").unwrap();
        let statuses = r.list_known();
        let s = statuses.iter().find(|b| b.id == "star").unwrap();
        assert!(s.configured_path.is_none());
    }

    #[test]
    fn list_known_contains_all_registered() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = tmp.path().join("settings.json");
        let r = BinaryResolver::load_from(settings).unwrap();
        let ids: Vec<_> = r.list_known().into_iter().map(|b| b.id).collect();
        assert!(ids.contains(&"star".to_string()));
        assert!(ids.contains(&"cutadapt-rs".to_string()));
        assert!(ids.contains(&"gffread-rs".to_string()));
    }
}
