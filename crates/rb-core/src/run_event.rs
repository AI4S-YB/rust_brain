use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    Progress { fraction: f64, message: String },
    Log { line: String, stream: LogStream },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_round_trip() {
        let ev = RunEvent::Progress {
            fraction: 0.5,
            message: "halfway".into(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        match back {
            RunEvent::Progress { fraction, message } => {
                assert_eq!(fraction, 0.5);
                assert_eq!(message, "halfway");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn log_stderr_round_trip() {
        let ev = RunEvent::Log {
            line: "STAR starting".into(),
            stream: LogStream::Stderr,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"stream\":\"stderr\""));
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        match back {
            RunEvent::Log { line, stream } => {
                assert_eq!(line, "STAR starting");
                assert_eq!(stream, LogStream::Stderr);
            }
            _ => panic!("wrong variant"),
        }
    }
}
