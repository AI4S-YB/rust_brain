//! Layered memory store (L0–L4).

pub mod crystallize;
pub mod layers;
pub mod recall;
pub mod store;

pub use crystallize::{
    crystallize_session, long_term_update, Layer, LongTermBody, SessionSummaryInput, UpdateResult,
};
pub use layers::{
    Archive, ArchiveOutcome, IndexEntry, Insight, Scope, SkillMeta, TodoEntry, WorkingCheckpoint,
};
pub use recall::{Bm25Recaller, CompositeRecaller, RecallCandidate, RecallResult, Recaller};
pub use store::MemoryStore;
