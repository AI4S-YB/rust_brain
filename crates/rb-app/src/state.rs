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
}

impl AppState {
    pub fn new(registry: ModuleRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
            runner: Arc::new(Mutex::new(None)),
            recent_projects: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
