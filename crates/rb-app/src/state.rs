use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::module::{Module, ModuleError, ModuleResult, ValidationError};
use rb_core::run_event::RunEvent;
use rb_core::runner::Runner;
use rb_plugin::PluginManifest;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::ai_state::AiState;

pub struct ModuleRegistry {
    modules: HashMap<String, Arc<dyn Module>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    pub fn register(&mut self, module: Arc<dyn Module>) {
        self.modules.insert(module.id().to_string(), module);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Module>> {
        self.modules.get(id).cloned()
    }

    #[allow(dead_code)]
    pub fn list_ids(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }

    /// Snapshot of every registered module — used by `list_modules` Tauri
    /// command so the frontend can render dynamic sidebar entries.
    #[allow(dead_code)]
    pub fn list_all(&self) -> Vec<Arc<dyn Module>> {
        self.modules.values().cloned().collect()
    }

    #[allow(dead_code)]
    pub fn remove(&mut self, id: &str) {
        self.modules.remove(id);
    }
}

pub struct AppState {
    pub registry: Arc<Mutex<ModuleRegistry>>,
    pub runner: Arc<Mutex<Option<Arc<Runner>>>>,
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
    pub plugins: Arc<Mutex<PluginDiagnostics>>,
    pub plugin_manifests: Arc<Mutex<HashMap<String, Arc<PluginManifest>>>>,
    #[allow(dead_code)]
    pub user_plugin_dir: PathBuf,
    pub ai: Arc<AiState>,
}

impl AppState {
    pub fn new(
        registry: ModuleRegistry,
        binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
        user_plugin_dir: PathBuf,
        ai: Arc<AiState>,
    ) -> Self {
        Self {
            registry: Arc::new(Mutex::new(registry)),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
            binary_resolver,
            plugins: Arc::new(Mutex::new(PluginDiagnostics::default())),
            plugin_manifests: Arc::new(Mutex::new(HashMap::new())),
            user_plugin_dir,
            ai,
        }
    }
}

/// Plugin ids tagged by source so the frontend can render badges + categories.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginSourceTag {
    pub id: String,
    pub source: String, // "bundled" | "user"
    pub origin_path: Option<PathBuf>,
    pub category: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub binary_id: String,
}

/// Plugin loader diagnostics surfaced in Settings.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PluginDiagnostics {
    pub loaded: Vec<PluginSourceTag>,
    pub errors: Vec<PluginErrorView>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginErrorView {
    pub source_label: String,
    pub message: String,
}

/// Wraps `ExternalToolModule` so the binary path is resolved fresh each run
/// against the live `BinaryResolver` — picking up Settings changes without
/// rebuilding the registry.
pub struct LazyResolvingPluginModule {
    manifest: Arc<PluginManifest>,
    binary_id: String,
    resolver: Arc<Mutex<BinaryResolver>>,
}

impl LazyResolvingPluginModule {
    pub fn new(
        manifest: Arc<PluginManifest>,
        binary_id: String,
        resolver: Arc<Mutex<BinaryResolver>>,
    ) -> Self {
        Self {
            manifest,
            binary_id,
            resolver,
        }
    }

    #[allow(dead_code)]
    pub fn manifest_arc(&self) -> Arc<PluginManifest> {
        self.manifest.clone()
    }
}

#[async_trait::async_trait]
impl Module for LazyResolvingPluginModule {
    fn id(&self) -> &str {
        &self.manifest.id
    }
    fn name(&self) -> &str {
        &self.manifest.name
    }
    fn params_schema(&self) -> Option<serde_json::Value> {
        Some(rb_plugin::schema::derive_json_schema(&self.manifest))
    }
    fn ai_hint(&self, lang: &str) -> String {
        let s = self.manifest.strings.as_ref();
        match lang {
            "zh" => s.and_then(|s| s.ai_hint_zh.clone()),
            _ => s.and_then(|s| s.ai_hint_en.clone()),
        }
        .or_else(|| s.and_then(|s| s.ai_hint_en.clone()))
        .or_else(|| self.manifest.description.clone())
        .unwrap_or_default()
    }
    fn validate(&self, params: &serde_json::Value) -> Vec<ValidationError> {
        rb_plugin::validate::validate_against_manifest(&self.manifest, params)
    }
    async fn run(
        &self,
        params: &serde_json::Value,
        project_dir: &Path,
        events_tx: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> Result<ModuleResult, ModuleError> {
        let path = {
            let r = self.resolver.lock().await;
            r.resolve(&self.binary_id)
                .map_err(|e| ModuleError::ToolError(e.to_string()))?
        };
        let inner = rb_plugin::ExternalToolModule::new(self.manifest.clone(), path);
        inner.run(params, project_dir, events_tx, cancel).await
    }
}
