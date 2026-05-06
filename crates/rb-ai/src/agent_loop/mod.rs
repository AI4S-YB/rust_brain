//! Perceive→reason→execute→record main loop.

pub mod execute;
pub mod perceive;
pub mod reason;
pub mod types;

pub use execute::{execute_call, ApprovalVerdict, ExecCtx};
pub use types::{AgentEvent, AgentSession, SharedSession};
