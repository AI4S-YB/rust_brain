use std::path::PathBuf;
use std::sync::Arc;

use rb_ai::config::keyring::KeyStore;
use rb_ai::config::AiConfig;
use tokio::sync::Mutex;

/// Holds everything the AI provider layer needs across the app lifetime.
///
/// - `keystore` — opaque KeyStore implementation (keyring or encrypted file).
/// - `config_path` + `config` — persisted AiConfig (Mutex for in-place updates).
///
/// Note: chat session / orchestrator / per-turn state has been removed in
/// preparation for the self-evolving agent rewrite. The `agent_loop`,
/// `memory`, and `sandbox` modules in rb-ai will own their own state in
/// later tasks.
pub struct AiState {
    pub keystore: Arc<dyn KeyStore>,
    pub config_path: PathBuf,
    pub config: Mutex<AiConfig>,
}
