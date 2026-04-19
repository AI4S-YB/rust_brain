use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{oneshot, Mutex};

use crate::error::AiError;

#[derive(Debug)]
pub enum PlanDecision {
    Approve { edited_args: Option<Value> },
    Reject { reason: Option<String> },
}

#[derive(Default)]
pub struct PendingPlans {
    waiters: HashMap<String, oneshot::Sender<PlanDecision>>,
}

#[derive(Clone, Default)]
pub struct PlanCardRegistry {
    inner: Arc<Mutex<PendingPlans>>,
}

impl PlanCardRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pending plan card. The returned receiver resolves when the
    /// user approves or rejects the call; if the controller is dropped the
    /// sender is dropped too and the receiver fails (orchestrator treats that
    /// as an implicit reject).
    pub async fn register(&self, call_id: String) -> oneshot::Receiver<PlanDecision> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().await.waiters.insert(call_id, tx);
        rx
    }

    pub async fn approve(&self, call_id: &str, edited_args: Option<Value>) -> Result<(), AiError> {
        let tx = self
            .inner
            .lock()
            .await
            .waiters
            .remove(call_id)
            .ok_or_else(|| AiError::InvalidState(format!("no pending plan for {call_id}")))?;
        tx.send(PlanDecision::Approve { edited_args })
            .map_err(|_| AiError::InvalidState("plan waiter dropped".into()))
    }

    pub async fn reject(&self, call_id: &str, reason: Option<String>) -> Result<(), AiError> {
        let tx = self
            .inner
            .lock()
            .await
            .waiters
            .remove(call_id)
            .ok_or_else(|| AiError::InvalidState(format!("no pending plan for {call_id}")))?;
        tx.send(PlanDecision::Reject { reason })
            .map_err(|_| AiError::InvalidState("plan waiter dropped".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_then_approve_resolves_waiter() {
        let reg = PlanCardRegistry::new();
        let rx = reg.register("tc_1".into()).await;
        reg.approve("tc_1", Some(serde_json::json!({"foo": "bar"})))
            .await
            .unwrap();
        let dec = rx.await.unwrap();
        match dec {
            PlanDecision::Approve { edited_args } => {
                assert_eq!(edited_args.unwrap()["foo"], "bar");
            }
            _ => panic!("expected approve"),
        }
    }

    #[tokio::test]
    async fn register_then_reject_resolves_waiter() {
        let reg = PlanCardRegistry::new();
        let rx = reg.register("tc_2".into()).await;
        reg.reject("tc_2", Some("nope".into())).await.unwrap();
        let dec = rx.await.unwrap();
        match dec {
            PlanDecision::Reject { reason } => assert_eq!(reason.as_deref(), Some("nope")),
            _ => panic!("expected reject"),
        }
    }

    #[tokio::test]
    async fn double_approve_is_invalid_state() {
        let reg = PlanCardRegistry::new();
        let _rx = reg.register("tc_3".into()).await;
        reg.approve("tc_3", None).await.unwrap();
        let err = reg.approve("tc_3", None).await.unwrap_err();
        assert!(matches!(err, AiError::InvalidState(_)));
    }
}
