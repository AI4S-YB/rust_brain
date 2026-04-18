use rb_core::module::Module;
use rb_core::runner::Runner;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

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

    pub fn list_ids(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }
}

pub struct AppState {
    pub registry: Arc<ModuleRegistry>,
    pub runner: Arc<Mutex<Option<Runner>>>,
    pub recent_projects: Arc<Mutex<Vec<PathBuf>>>,
    pub binary_resolver: Arc<Mutex<rb_core::binary::BinaryResolver>>,
}

impl AppState {
    pub fn new(registry: ModuleRegistry) -> Self {
        let resolver = rb_core::binary::BinaryResolver::load().unwrap_or_else(|e| {
            eprintln!(
                "warning: failed to load binary settings ({}); using defaults",
                e
            );
            rb_core::binary::BinaryResolver::with_defaults_at(
                rb_core::binary::BinaryResolver::default_settings_path(),
            )
        });
        Self {
            registry: Arc::new(registry),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
            binary_resolver: Arc::new(Mutex::new(resolver)),
        }
    }
}
