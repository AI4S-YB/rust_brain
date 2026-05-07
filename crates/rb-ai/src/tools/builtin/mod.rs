//! Built-in (host-agnostic) tools: file/code_run/web/memory/ask_user.
//! Bio-specific tools (`project_state`, `run_<module>`) live in the
//! sibling `rb-ai-bio` crate and are registered separately by the host.

pub mod ask_user;
pub mod code_run;
pub mod file;
pub mod memory_tools;
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
