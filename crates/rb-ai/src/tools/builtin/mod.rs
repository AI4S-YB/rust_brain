//! Built-in tools (non module-derived, non skill).
//!
//! Houses project-state tools (file/run/asset listings, table previews, run
//! status), atomic file_read/write/patch helpers, code_run, web_scan,
//! memory-mutating tools (recall_memory, update_working_checkpoint,
//! start_long_term_update, task_done) and ask_user.

pub mod ask_user;
pub mod code_run;
pub mod file;
pub mod memory_tools;
pub mod project_state;
pub mod web;

use crate::tools::ToolRegistry;

/// Register every builtin tool into the given registry. Called once at
/// agent boot.
pub fn register_all(registry: &mut ToolRegistry) {
    file::register(registry);
    project_state::register(registry);
    code_run::register(registry);
    web::register(registry);
    memory_tools::register(registry);
    ask_user::register(registry);
}
