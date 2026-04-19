//! AI orchestration, provider abstraction, and chat session persistence.
//!
//! Depends on `rb-core` for `ModuleRegistry` and `Runner`; does not depend on
//! any Tauri or UI code, so it can be reused in a headless CLI or MCP server.

pub mod config;
pub mod error;
pub mod orchestrator;
pub mod provider;
pub mod session;
pub mod tools;

pub use error::AiError;
