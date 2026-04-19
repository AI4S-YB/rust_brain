use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use rb_ai::orchestrator::{run_turn, ChatStreamEvent, OrchestratorCtx, PlanCardRegistry};
use rb_ai::provider::{ChatProvider, ChatRequest, FinishReason, ProviderError, ProviderEvent};
use rb_ai::session::{ChatSession, Message};
use rb_ai::tools::{builtin, ToolRegistry};
use rb_core::binary::BinaryResolver;
use rb_core::cancel::CancellationToken;
use rb_core::project::Project;
use rb_core::runner::Runner;
use tempfile::tempdir;
use tokio::sync::{mpsc, Mutex};

/// MockProvider replays a scripted sequence of events per turn.
struct MockProvider {
    script: StdMutex<Vec<Vec<ProviderEvent>>>,
}

impl MockProvider {
    fn new(turns: Vec<Vec<ProviderEvent>>) -> Self {
        Self {
            script: StdMutex::new(turns),
        }
    }
}

#[async_trait]
impl ChatProvider for MockProvider {
    fn id(&self) -> &str {
        "mock"
    }
    async fn send(
        &self,
        _req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>,
        _c: CancellationToken,
    ) -> Result<(), ProviderError> {
        let next = self.script.lock().unwrap().remove(0);
        for ev in next {
            let _ = sink.send(ev).await;
        }
        Ok(())
    }
}

type SaveFn = Arc<
    dyn Fn(ChatSession) -> futures_util::future::BoxFuture<'static, Result<(), rb_ai::AiError>>
        + Send
        + Sync,
>;

fn noop_save_fn() -> SaveFn {
    Arc::new(|_s| Box::pin(async move { Ok(()) }))
}

#[tokio::test]
async fn turn_without_tool_calls_emits_text_then_done() {
    let tmp = tempdir().unwrap();
    let project = Arc::new(Mutex::new(Project::create("t", tmp.path()).unwrap()));
    let runner = Arc::new(Runner::new(project.clone()));
    let resolver = Arc::new(Mutex::new(BinaryResolver::with_defaults_at(
        tmp.path().join("bin.json"),
    )));
    let mut tools = ToolRegistry::new();
    builtin::register_all(&mut tools);

    let provider = Arc::new(MockProvider::new(vec![vec![
        ProviderEvent::TextDelta("Hel".into()),
        ProviderEvent::TextDelta("lo".into()),
        ProviderEvent::Finish(FinishReason::Stop),
    ]]));

    let ctx = OrchestratorCtx {
        project: project.clone(),
        runner: runner.clone(),
        binary_resolver: resolver.clone(),
        tools: Arc::new(tools),
        provider,
        model: "m".into(),
        temperature: 0.0,
        plans: PlanCardRegistry::new(),
        lang: "en".into(),
    };
    let session = Arc::new(Mutex::new(ChatSession::new("s1".into(), "t".into(), None)));
    let (tx, mut rx) = mpsc::channel(64);

    run_turn(
        &ctx,
        session.clone(),
        "hi".into(),
        tx,
        CancellationToken::new(),
        noop_save_fn(),
    )
    .await
    .unwrap();

    let mut texts = String::new();
    let mut saw_done = false;
    while let Some(ev) = rx.recv().await {
        match ev {
            ChatStreamEvent::Text { delta, .. } => texts.push_str(&delta),
            ChatStreamEvent::Done { .. } => {
                saw_done = true;
                break;
            }
            _ => {}
        }
    }
    assert_eq!(texts, "Hello");
    assert!(saw_done);

    let msgs = &session.lock().await.messages;
    assert_eq!(msgs.len(), 2);
    assert!(matches!(msgs[0], Message::User { .. }));
    assert!(matches!(msgs[1], Message::Assistant { .. }));
}

#[tokio::test]
async fn read_risk_tool_call_feeds_result_back_to_provider() {
    let tmp = tempdir().unwrap();
    let project = Arc::new(Mutex::new(Project::create("t", tmp.path()).unwrap()));
    let runner = Arc::new(Runner::new(project.clone()));
    let resolver = Arc::new(Mutex::new(BinaryResolver::with_defaults_at(
        tmp.path().join("bin.json"),
    )));
    let mut tools = ToolRegistry::new();
    builtin::register_all(&mut tools);

    // Turn 1: request get_project_info. Turn 2: say "Project is t." after result.
    let provider = Arc::new(MockProvider::new(vec![
        vec![
            ProviderEvent::ToolCall {
                id: "tc_a".into(),
                name: "get_project_info".into(),
                args: serde_json::json!({}),
            },
            ProviderEvent::Finish(FinishReason::ToolCalls),
        ],
        vec![
            ProviderEvent::TextDelta("Project is t.".into()),
            ProviderEvent::Finish(FinishReason::Stop),
        ],
    ]));

    let ctx = OrchestratorCtx {
        project: project.clone(),
        runner: runner.clone(),
        binary_resolver: resolver.clone(),
        tools: Arc::new(tools),
        provider,
        model: "m".into(),
        temperature: 0.0,
        plans: PlanCardRegistry::new(),
        lang: "en".into(),
    };
    let session = Arc::new(Mutex::new(ChatSession::new("s2".into(), "t".into(), None)));
    let (tx, mut rx) = mpsc::channel(64);

    run_turn(
        &ctx,
        session.clone(),
        "status".into(),
        tx,
        CancellationToken::new(),
        noop_save_fn(),
    )
    .await
    .unwrap();

    let mut saw_tool_result = false;
    let mut text = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            ChatStreamEvent::ToolResult { result, .. } => {
                saw_tool_result = true;
                assert_eq!(result["name"], "t");
            }
            ChatStreamEvent::Text { delta, .. } => text.push_str(&delta),
            ChatStreamEvent::Done { .. } => break,
            _ => {}
        }
    }
    assert!(saw_tool_result);
    assert_eq!(text, "Project is t.");
}
