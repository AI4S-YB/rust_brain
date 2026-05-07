//! Built-in tools (non module-derived, non skill).
//!
//! Houses host-agnostic tools (file/code_run/web/memory/ask_user) and the
//! bio-specific `project_state` family. Hosts call `register_all` for the
//! generic set and (when running rust_brain projects) the gated
//! `project_state::register` for the bio-specific tools.

pub mod ask_user;
pub mod code_run;
pub mod file;
pub mod memory_tools;
#[cfg(feature = "bio-tools")]
pub mod project_state;
pub mod web;

use crate::tools::ToolRegistry;

/// Register the host-agnostic builtin tools. Called once at agent boot.
/// Bio-specific tools (`project_state`) are registered separately by the
/// host so they can pass in their concrete `Project`/`BinaryResolver`.
pub fn register_all(registry: &mut ToolRegistry) {
    file::register(registry);
    code_run::register(registry);
    web::register(registry);
    memory_tools::register(registry);
    ask_user::register(registry);
}
