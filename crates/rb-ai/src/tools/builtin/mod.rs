//! Built-in tools (non module-derived, non skill).
//!
//! Currently houses project-state tools (file/run/asset listings, table
//! previews, run status). Future tasks add atomic file_read/write/patch
//! into `file.rs`, code_run, web_scan, memory tools, ask_user, and a
//! skill loader as separate submodules.

pub mod file;
pub mod project_state;

use crate::tools::ToolRegistry;

/// Register every builtin tool into the given registry. Called once at
/// agent boot.
pub fn register_all(registry: &mut ToolRegistry) {
    project_state::register(registry);
    file::register(registry);
    // code_run, web, memory_tools, ask_user, skill loader: registered by
    // separate modules — see lib.rs orchestration.
}
