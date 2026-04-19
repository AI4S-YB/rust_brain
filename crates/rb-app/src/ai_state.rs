use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::config::keyring::KeyStore;
use rb_ai::config::AiConfig;
use rb_ai::orchestrator::PlanCardRegistry;
use rb_ai::tools::ToolRegistry;
use rb_core::cancel::CancellationToken;
use rb_core::module::Module;
use tokio::sync::Mutex;

/// Holds everything the AI chat layer needs across the app lifetime.
///
/// - `tools_by_lang` — prebuilt tool registries keyed by language (the
///   module-derived tool descriptions differ per locale).
/// - `keystore` — opaque KeyStore implementation (keyring or encrypted file).
/// - `config_path` + `config` — persisted AiConfig (Mutex for in-place updates).
/// - `plans` — shared plan-card rendezvous between the orchestrator and
///   `chat_approve_tool` / `chat_reject_tool` commands.
/// - `active_turns` — per-session cancellation tokens so `chat_cancel_turn`
///   can interrupt in-flight provider streams.
pub struct AiState {
    pub tools_by_lang: HashMap<String, Arc<ToolRegistry>>,
    pub keystore: Arc<dyn KeyStore>,
    pub config_path: PathBuf,
    pub config: Mutex<AiConfig>,
    pub plans: PlanCardRegistry,
    pub active_turns: Mutex<HashMap<String, CancellationToken>>,
}

/// Build a ToolRegistry for a given language.
///
/// Composes the three tool sources defined in rb-ai::tools (builtin Read tools,
/// module-derived Run tools, Phase 3 stubs) into a single registry that the
/// orchestrator will hand to the provider each turn.
pub fn build_tool_registry(modules: &[Arc<dyn Module>], lang: &str) -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    rb_ai::tools::builtin::register_all(&mut reg);
    rb_ai::tools::module_derived::register_for_modules(&mut reg, modules, lang);
    rb_ai::tools::stubs::register_all(&mut reg);
    reg
}
