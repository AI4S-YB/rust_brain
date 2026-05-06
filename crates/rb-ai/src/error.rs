use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("cancelled")]
    Cancelled,
    #[error("config error: {0}")]
    Config(String),
    #[error("keyring error: {0}")]
    Keyring(String),
    #[error("provider not configured")]
    ProviderNotConfigured,
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("sandbox violation: {0}")]
    SandboxViolation(String),
    #[error("path escapes allowed root: {0}")]
    PathEscape(String),
    #[error("memory write failed: {0}")]
    MemoryWrite(String),
    #[error("agent already running for project {0}")]
    AgentAlreadyRunning(String),
}
