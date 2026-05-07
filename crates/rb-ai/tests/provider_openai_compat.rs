use rb_ai::provider::openai_compat::OpenAiCompatProvider;
use rb_ai::provider::{
    ChatProvider, ChatRequest, FinishReason, ProviderEvent, ProviderMessage, ProviderToolCall,
    ThinkingConfig,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(body.as_bytes().to_vec(), "text/event-stream")
}

fn basic_req(model: &str) -> ChatRequest {
    ChatRequest {
        model: model.into(),
        system: "sys".into(),
        messages: vec![ProviderMessage::User {
            content: "hi".into(),
        }],
        tools: vec![],
        temperature: 0.0,
        thinking: ThinkingConfig::default(),
    }
}

#[tokio::test]
async fn openai_compat_streams_text_and_finishes() {
    let server = MockServer::start().await;
    let body = "\
data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(sse(body))
        .mount(&server)
        .await;

    let p = OpenAiCompatProvider::new(server.uri(), "test-key".into());
    let (tx, mut rx) = mpsc::channel(16);
    let cancel = CancellationToken::new();
    p.send(basic_req("m"), tx, cancel).await.unwrap();

    let mut texts = String::new();
    let mut finish = None;
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::TextDelta(s) => texts.push_str(&s),
            ProviderEvent::Finish(r) => finish = Some(r),
            _ => {}
        }
    }
    assert_eq!(texts, "Hello");
    assert!(matches!(finish, Some(FinishReason::Stop)));
}

#[tokio::test]
async fn openai_compat_emits_tool_calls() {
    let server = MockServer::start().await;
    // tool_calls are streamed in multiple deltas; the provider must accumulate
    // arguments by `index` before emitting a single ProviderEvent::ToolCall.
    let body = "\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"tc_1\",\"type\":\"function\",\"function\":{\"name\":\"list_project_files\",\"arguments\":\"\"}}]}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"subdir\\\":\\\"data\\\"}\"}}]}}]}\n\n\
data: {\"choices\":[{\"finish_reason\":\"tool_calls\"}]}\n\n\
data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(body))
        .mount(&server)
        .await;

    let p = OpenAiCompatProvider::new(server.uri(), "k".into());
    let (tx, mut rx) = mpsc::channel(16);
    p.send(basic_req("m"), tx, CancellationToken::new())
        .await
        .unwrap();

    let mut saw_tool = None;
    let mut finish = None;
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::ToolCall { id, name, args } => {
                saw_tool = Some((id, name, args));
            }
            ProviderEvent::Finish(r) => finish = Some(r),
            _ => {}
        }
    }
    let (id, name, args) = saw_tool.expect("tool call expected");
    assert_eq!(id, "tc_1");
    assert_eq!(name, "list_project_files");
    assert_eq!(args["subdir"], "data");
    assert!(matches!(finish, Some(FinishReason::ToolCalls)));
}

#[tokio::test]
async fn openai_compat_streams_reasoning_content() {
    let server = MockServer::start().await;
    let body = "\
data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think \"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"step\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"final\"}}]}\n\n\
data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(body))
        .mount(&server)
        .await;

    let p = OpenAiCompatProvider::new(server.uri(), "k".into());
    let (tx, mut rx) = mpsc::channel(16);
    let mut req = basic_req("m");
    req.thinking = ThinkingConfig {
        enabled: true,
        reasoning_effort: Some("high".into()),
    };
    p.send(req, tx, CancellationToken::new()).await.unwrap();

    let mut reasoning = String::new();
    let mut text = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            ProviderEvent::ReasoningDelta(s) => reasoning.push_str(&s),
            ProviderEvent::TextDelta(s) => text.push_str(&s),
            _ => {}
        }
    }
    assert_eq!(reasoning, "think step");
    assert_eq!(text, "final");
}

#[tokio::test]
async fn openai_compat_sends_thinking_fields_and_reasoning_history() {
    let server = MockServer::start().await;
    let body = "\
data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n\
data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(sse(body))
        .mount(&server)
        .await;

    let p = OpenAiCompatProvider::new(server.uri(), "k".into());
    let (tx, _rx) = mpsc::channel(16);
    let mut req = basic_req("deepseek-v4-pro");
    req.thinking = ThinkingConfig {
        enabled: true,
        reasoning_effort: Some("high".into()),
    };
    req.messages.push(ProviderMessage::Assistant {
        content: "".into(),
        reasoning_content: Some("previous reasoning".into()),
        tool_calls: vec![ProviderToolCall {
            id: "tc_1".into(),
            name: "get_project_info".into(),
            args: serde_json::json!({}),
        }],
    });
    p.send(req, tx, CancellationToken::new()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(sent["thinking"]["type"], "enabled");
    assert_eq!(sent["reasoning_effort"], "high");
    assert!(sent.get("temperature").is_none());
    assert_eq!(
        sent["messages"][2]["reasoning_content"],
        "previous reasoning"
    );
}

#[tokio::test]
async fn openai_compat_maps_401_to_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string("{\"error\":{\"message\":\"bad key\"}}"),
        )
        .mount(&server)
        .await;
    let p = OpenAiCompatProvider::new(server.uri(), "bad".into());
    let (tx, _rx) = mpsc::channel(4);
    let err = p
        .send(basic_req("m"), tx, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, rb_ai::provider::ProviderError::Auth(_)));
}
