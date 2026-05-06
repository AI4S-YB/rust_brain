//! web_scan — HTTP GET. Returns trimmed body text. Network whitelist /
//! logging is enforced by the agent_loop wrapper; this tool is the raw
//! reqwest call.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

pub fn register(reg: &mut ToolRegistry) {
    reg.register(ToolEntry {
        def: def(),
        executor: std::sync::Arc::new(WebScanExec),
    });
}

fn def() -> ToolDef {
    ToolDef {
        name: "web_scan".into(),
        description: "HTTP GET; returns body trimmed to max_bytes.".into(),
        risk: RiskLevel::RunLow,
        params: json!({
            "type": "object",
            "properties": {
                "url": {"type": "string"},
                "max_bytes": {"type": "integer", "default": 65536},
                "headers": {"type": "object", "additionalProperties": {"type": "string"}}
            },
            "required": ["url"]
        }),
    }
}

struct WebScanExec;
#[async_trait]
impl ToolExecutor for WebScanExec {
    async fn execute(&self, args: &Value, _: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArgs("url required".into()))?;
        let max = args
            .get("max_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(65536) as usize;
        let mut req = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .get(url);
        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(s) = v.as_str() {
                    req = req.header(k, s);
                }
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ToolError::Execution(format!("http: {e}")))?;
        let status = resp.status().as_u16();
        let mut body = resp
            .bytes()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?
            .to_vec();
        let truncated = body.len() > max;
        if truncated {
            body.truncate(max);
        }
        let text = String::from_utf8_lossy(&body).into_owned();
        Ok(ToolOutput::Value(json!({
            "status": status,
            "truncated": truncated,
            "body": text
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx(root: &std::path::Path) -> ToolContext<'static> {
        let project = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::project::Project::create("t", root).unwrap(),
        ))));
        let runner = Box::leak(Box::new(std::sync::Arc::new(rb_core::runner::Runner::new(
            project.clone(),
        ))));
        let binres = Box::leak(Box::new(std::sync::Arc::new(tokio::sync::Mutex::new(
            rb_core::binary::BinaryResolver::with_defaults_at(root.join("binaries.json")),
        ))));
        ToolContext {
            project,
            runner,
            binary_resolver: binres,
        }
    }

    #[tokio::test]
    async fn web_scan_returns_status_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/data"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let exec = WebScanExec;
        let out = exec
            .execute(
                &json!({"url": format!("{}/data", server.uri())}),
                ctx(tmp.path()),
            )
            .await
            .unwrap();
        let ToolOutput::Value(v) = out;
        assert_eq!(v["status"], 200);
        assert_eq!(v["body"].as_str().unwrap(), "hello");
    }
}
