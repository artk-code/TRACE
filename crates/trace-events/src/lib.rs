use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputEncoding {
    Utf8,
    Base64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerOutputPayload {
    pub stream: OutputStream,
    pub encoding: OutputEncoding,
    pub chunk: String,
    pub chunk_index: u64,
    #[serde(rename = "final", default)]
    pub final_chunk: bool,
    #[serde(default)]
    pub worker_id: Option<String>,
    #[serde(default)]
    pub lease_epoch: Option<u64>,
    #[serde(default)]
    pub epoch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "task.claimed")]
    TaskClaimed,
    #[serde(rename = "task.renewed")]
    TaskRenewed,
    #[serde(rename = "task.released")]
    TaskReleased,
    #[serde(rename = "verdict.recorded")]
    VerdictRecorded,
    #[serde(rename = "run.started")]
    RunStarted,
    #[serde(rename = "runner.output")]
    RunnerOutput,
    #[serde(rename = "changeset.created")]
    ChangesetCreated,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewTraceEvent {
    pub global_seq: Option<u64>,
    pub ts: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub kind: EventKind,
    pub payload: Value,
}

impl NewTraceEvent {
    pub fn persist_with_global_seq(self, global_seq: u64) -> TraceEvent {
        TraceEvent {
            global_seq,
            ts: self.ts,
            task_id: self.task_id,
            run_id: self.run_id,
            kind: self.kind,
            payload: self.payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceEvent {
    pub global_seq: u64,
    pub ts: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub kind: EventKind,
    pub payload: Value,
}

impl TraceEvent {
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
    }

    pub fn validate(&self) -> Result<(), EventValidationError> {
        if self.kind == EventKind::RunnerOutput {
            validate_runner_output_payload(&self.payload)?;
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum EventValidationError {
    #[error("runner.output payload invalid: {0}")]
    InvalidRunnerOutput(#[from] serde_json::Error),
}

pub fn validate_runner_output_payload(
    payload: &Value,
) -> Result<RunnerOutputPayload, EventValidationError> {
    serde_json::from_value(payload.clone()).map_err(EventValidationError::from)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        validate_runner_output_payload, EventKind, NewTraceEvent, OutputEncoding, OutputStream,
        RunnerOutputPayload,
    };

    #[test]
    fn test_new_trace_event_global_seq_is_none_before_persist() {
        let event = NewTraceEvent {
            global_seq: None,
            ts: "2026-02-28T05:20:18.123Z".to_string(),
            task_id: "TASK-42".to_string(),
            run_id: Some("RUN-13".to_string()),
            kind: EventKind::RunStarted,
            payload: json!({}),
        };

        assert_eq!(event.global_seq, None);
    }

    #[test]
    fn test_trace_event_global_seq_is_set_after_persist() {
        let event = NewTraceEvent {
            global_seq: None,
            ts: "2026-02-28T05:20:18.123Z".to_string(),
            task_id: "TASK-42".to_string(),
            run_id: Some("RUN-13".to_string()),
            kind: EventKind::RunStarted,
            payload: json!({}),
        };

        let persisted = event.persist_with_global_seq(1842);
        assert_eq!(persisted.global_seq, 1842);
    }

    #[test]
    fn test_runner_output_requires_encoding_field() {
        let payload = json!({
            "stream": "stdout",
            "chunk": "abc",
            "chunk_index": 1
        });

        let result = validate_runner_output_payload(&payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_runner_output_json_with_encoding_passes_validation() {
        let payload = RunnerOutputPayload {
            stream: OutputStream::Stdout,
            encoding: OutputEncoding::Utf8,
            chunk: "done".to_string(),
            chunk_index: 0,
            final_chunk: true,
            worker_id: None,
            lease_epoch: None,
            epoch: None,
        };

        let value = serde_json::to_value(payload).expect("payload should serialize");
        let result = validate_runner_output_payload(&value);
        assert!(result.is_ok());
    }
}
