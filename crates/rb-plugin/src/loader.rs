//! Plugin loader — filled in Task 6.

use crate::manifest::PluginManifest;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSource { Bundled, User }

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub source: PluginSource,
    pub origin_path: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    pub by_id: HashMap<String, LoadedPlugin>,
}

pub fn load_plugins(
    _bundled: &include_dir::Dir<'_>,
    _user_dir: Option<&std::path::Path>,
) -> PluginRegistry {
    PluginRegistry::default()
}
