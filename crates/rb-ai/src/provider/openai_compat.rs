use std::time::Duration;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::Value;
use tokio::sync::mpsc;

use tokio_util::sync::CancellationToken;

use super::{
    ChatProvider, ChatRequest, FinishReason, ProviderError, ProviderEvent, ProviderMessage,
};

pub struct OpenAiCompatProvider {
    base_url: String,
    api_key: String,
    client: reqwest::Client,
    direct_client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");
        let direct_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .no_proxy()
            .build()
            .expect("reqwest direct client");
        Self {
            base_url,
            api_key,
            client,
            direct_client,
        }
    }

    fn is_deepseek_endpoint(&self) -> bool {
        self.base_url
            .trim()
            .to_ascii_lowercase()
            .contains("api.deepseek.com")
    }
}

fn messages_to_openai(messages: &[ProviderMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| match m {
            ProviderMessage::User { content } => {
                serde_json::json!({ "role": "user", "content": content })
            }
            ProviderMessage::Assistant {
                content,
                reasoning_content,
                tool_calls,
            } => {
                let mut obj = serde_json::json!({ "role": "assistant", "content": content });
                if let Some(reasoning) = reasoning_content.as_deref().filter(|s| !s.is_empty()) {
                    obj["reasoning_content"] = Value::String(reasoning.to_string());
                }
                if !tool_calls.is_empty() {
                    obj["tool_calls"] = Value::Array(
                        tool_calls
                            .iter()
                            .map(|tc| {
                                serde_json::json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.args.to_string()
                                    }
                                })
                            })
                            .collect(),
                    );
                }
                obj
            }
            ProviderMessage::Tool {
                call_id,
                name,
                result,
            } => serde_json::json!({
                "role": "tool",
                "tool_call_id": call_id,
                "name": name,
                "content": result,
            }),
        })
        .collect()
}

fn tools_to_openai(tools: &[crate::tools::ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.params,
                }
            })
        })
        .collect()
}

#[async_trait]
impl ChatProvider for OpenAiCompatProvider {
    fn id(&self) -> &str {
        "openai-compat"
    }

    async fn send(
        &self,
        req: ChatRequest,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ProviderError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut messages = vec![serde_json::json!({"role":"system","content": req.system})];
        messages.extend(messages_to_openai(&req.messages));
        let mut body = serde_json::json!({
            "model": req.model,
            "messages": messages,
            "stream": true,
        });
        if req.thinking.enabled {
            body["thinking"] = serde_json::json!({ "type": "enabled" });
            if let Some(effort) = req
                .thinking
                .reasoning_effort
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                body["reasoning_effort"] = Value::String(effort.to_string());
            }
        } else {
            body["temperature"] = serde_json::json!(req.temperature);
        }
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(tools_to_openai(&req.tools));
        }

        let client = if self.is_deepseek_endpoint() {
            &self.direct_client
        } else {
            &self.client
        };
        let resp = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(text),
                429 => ProviderError::RateLimited(text),
                _ => ProviderError::Http(format!("{status}: {text}")),
            });
        }

        // Buffers for streaming tool-call assembly keyed by index.
        #[derive(Default)]
        struct ToolBuf {
            id: String,
            name: String,
            args: String,
        }
        let mut tool_bufs: std::collections::BTreeMap<u64, ToolBuf> = Default::default();
        let mut emitted_finish = false;

        let mut stream = resp.bytes_stream().eventsource();
        while let Some(ev) = stream.next().await {
            if cancel.is_cancelled() {
                let _ = sink
                    .send(ProviderEvent::Finish(FinishReason::Error(
                        "cancelled".into(),
                    )))
                    .await;
                return Err(ProviderError::Cancelled);
            }
            let ev = ev.map_err(|e| ProviderError::Malformed(e.to_string()))?;
            if ev.data.trim() == "[DONE]" {
                break;
            }
            let v: Value = serde_json::from_str(&ev.data)
                .map_err(|e| ProviderError::Malformed(format!("bad json: {e}")))?;
            let choice = v["choices"].get(0).cloned().unwrap_or(Value::Null);
            let delta = choice.get("delta").cloned().unwrap_or(Value::Null);

            if let Some(s) = delta.get("content").and_then(|c| c.as_str()) {
                if !s.is_empty() {
                    let _ = sink.send(ProviderEvent::TextDelta(s.to_string())).await;
                }
            }
            if let Some(s) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
                if !s.is_empty() {
                    let _ = sink
                        .send(ProviderEvent::ReasoningDelta(s.to_string()))
                        .await;
                }
            }
            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                for tc in tcs {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                    let buf = tool_bufs.entry(idx).or_default();
                    if let Some(id) = tc.get("id").and_then(|s| s.as_str()) {
                        buf.id = id.to_string();
                    }
                    if let Some(name) = tc.pointer("/function/name").and_then(|s| s.as_str()) {
                        buf.name = name.to_string();
                    }
                    if let Some(args) = tc.pointer("/function/arguments").and_then(|s| s.as_str()) {
                        buf.args.push_str(args);
                    }
                }
            }
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                for (_, buf) in std::mem::take(&mut tool_bufs) {
                    let args: Value = serde_json::from_str(&buf.args).unwrap_or(Value::Null);
                    let _ = sink
                        .send(ProviderEvent::ToolCall {
                            id: buf.id,
                            name: buf.name,
                            args,
                        })
                        .await;
                }
                let reason = match fr {
                    "stop" => FinishReason::Stop,
                    "tool_calls" => FinishReason::ToolCalls,
                    "length" => FinishReason::Length,
                    other => FinishReason::Error(other.into()),
                };
                let _ = sink.send(ProviderEvent::Finish(reason)).await;
                emitted_finish = true;
            }
        }
        if !emitted_finish {
            let _ = sink.send(ProviderEvent::Finish(FinishReason::Stop)).await;
        }
        Ok(())
    }
}
