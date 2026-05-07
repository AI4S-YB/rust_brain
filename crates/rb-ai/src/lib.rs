//! Self-evolving agent core: provider abstraction, layered memory,
//! sandboxed tool execution, perceive→reason→execute→record loop.
//!
//! Host-agnostic — no dependence on the rust_brain workflow model. The
//! sibling `rb-ai-bio` crate bridges this framework with `rb-core`'s
//! `Module`/`Project`/`Runner` types.

pub mod agent_loop;
pub mod config;
pub mod error;
pub mod memory;
pub mod provider;
pub mod sandbox;
pub mod subprocess;
pub mod tools;

pub use error::AiError;
pub use memory::{
    Archive, ArchiveOutcome, Bm25Recaller, CompositeRecaller, IndexEntry, Insight, MemoryStore,
    Recaller, Scope, SkillMeta, TodoEntry, WorkingCheckpoint,
};
pub use sandbox::{Bucket, Decision, NetLogger, PolicyMode, SandboxPolicy};
