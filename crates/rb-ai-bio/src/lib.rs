//! Bio-specific agent tools bridging `rb-ai` (the generic agent
//! framework) with `rb-core` (the rust_brain workflow model).
//!
//! - [`project_state`] — read-tier tools the agent uses to inspect
//!   `Project` state: list inputs/samples/assets/runs, table previews,
//!   binary registry.
//! - [`module_derived`] — wraps each `Module` as a `run_<id>` tool the
//!   LLM can invoke.
//!
//! Hosts (e.g. `rb-app`) register both alongside the generic
//! `rb_ai::tools::builtin::register_all`.

pub mod module_derived;
pub mod project_state;
