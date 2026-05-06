//! AI orchestration, provider abstraction, and persistent agent memory.
//!
//! Depends on `rb-core` for `ModuleRegistry`, `Runner`, `Project`; does not
//! depend on Tauri.

pub mod agent_loop;
pub mod config;
pub mod error;
pub mod memory;
pub mod provider;
pub mod sandbox;
pub mod tools;

pub use error::AiError;
