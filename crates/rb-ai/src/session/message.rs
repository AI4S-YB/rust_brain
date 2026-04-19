use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        content: String,
    },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        interrupted: bool,
    },
    Tool {
        call_id: String,
        name: String,
        result: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub args: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_without_tool_calls_does_not_serialize_field() {
        let m = Message::Assistant {
            content: "hi".into(),
            tool_calls: vec![],
            interrupted: false,
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(
            !s.contains("tool_calls"),
            "empty tool_calls must be skipped"
        );
        assert!(
            !s.contains("interrupted"),
            "false interrupted must be skipped"
        );
    }

    #[test]
    fn legacy_assistant_message_without_new_fields_still_loads() {
        let legacy = r#"{"role":"assistant","content":"old"}"#;
        let m: Message = serde_json::from_str(legacy).unwrap();
        assert_eq!(
            m,
            Message::Assistant {
                content: "old".into(),
                tool_calls: vec![],
                interrupted: false
            }
        );
    }

    #[test]
    fn tool_message_roundtrips() {
        let m = Message::Tool {
            call_id: "tc_x".into(),
            name: "ls".into(),
            result: serde_json::json!({ "run_id": "abc" }),
        };
        let s = serde_json::to_string(&m).unwrap();
        let back: Message = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }
}
