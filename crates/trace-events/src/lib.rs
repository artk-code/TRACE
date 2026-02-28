#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputEncoding {
    Utf8,
    Base64,
}

impl OutputEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf8",
            Self::Base64 => "base64",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerOutputPayload {
    pub stream: OutputStream,
    pub encoding: OutputEncoding,
    pub chunk: String,
    pub chunk_index: u64,
    pub final_chunk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    TaskClaimed,
    TaskRenewed,
    TaskReleased,
    VerdictRecorded,
    RunStarted,
    RunnerOutput,
    ChangesetCreated,
    Unknown(String),
}

impl EventKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::TaskClaimed => "task.claimed",
            Self::TaskRenewed => "task.renewed",
            Self::TaskReleased => "task.released",
            Self::VerdictRecorded => "verdict.recorded",
            Self::RunStarted => "run.started",
            Self::RunnerOutput => "runner.output",
            Self::ChangesetCreated => "changeset.created",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventPayload {
    RunnerOutput(RunnerOutputPayload),
    RawObject(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTraceEvent {
    pub global_seq: Option<u64>,
    pub ts: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub kind: EventKind,
    pub payload: EventPayload,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub global_seq: u64,
    pub ts: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub kind: EventKind,
    pub payload: EventPayload,
}

impl TraceEvent {
    pub fn to_json_line(&self) -> String {
        let run_id = self
            .run_id
            .as_ref()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .unwrap_or_else(|| "null".to_string());

        let payload = match &self.payload {
            EventPayload::RunnerOutput(output) => format!(
                "{{\"stream\":\"{}\",\"encoding\":\"{}\",\"chunk\":\"{}\",\"chunk_index\":{},\"final\":{}}}",
                output.stream.as_str(),
                output.encoding.as_str(),
                escape_json(&output.chunk),
                output.chunk_index,
                output.final_chunk,
            ),
            EventPayload::RawObject(raw) => raw.clone(),
        };

        format!(
            "{{\"global_seq\":{},\"ts\":\"{}\",\"task_id\":\"{}\",\"run_id\":{},\"kind\":\"{}\",\"payload\":{}}}",
            self.global_seq,
            escape_json(&self.ts),
            escape_json(&self.task_id),
            run_id,
            self.kind.as_str(),
            payload,
        )
    }
}

pub fn validate_runner_output_payload_json(raw_json: &str) -> Result<(), &'static str> {
    if !raw_json.contains("\"stream\"") {
        return Err("runner.output missing stream");
    }
    if !raw_json.contains("\"encoding\"") {
        return Err("runner.output missing encoding");
    }
    if !raw_json.contains("\"chunk\"") {
        return Err("runner.output missing chunk");
    }
    if !raw_json.contains("\"chunk_index\"") {
        return Err("runner.output missing chunk_index");
    }

    Ok(())
}

fn escape_json(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{
        EventKind, EventPayload, NewTraceEvent, OutputEncoding, OutputStream, RunnerOutputPayload,
        validate_runner_output_payload_json,
    };

    #[test]
    fn test_new_trace_event_global_seq_is_none_before_persist() {
        let event = NewTraceEvent {
            global_seq: None,
            ts: "2026-02-28T05:20:18.123Z".to_string(),
            task_id: "TASK-42".to_string(),
            run_id: Some("RUN-13".to_string()),
            kind: EventKind::RunStarted,
            payload: EventPayload::RawObject("{}".to_string()),
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
            payload: EventPayload::RawObject("{}".to_string()),
        };

        let persisted = event.persist_with_global_seq(1842);
        assert_eq!(persisted.global_seq, 1842);
    }

    #[test]
    fn test_runner_output_requires_encoding_field() {
        let raw_json = "{\"stream\":\"stdout\",\"chunk\":\"abc\",\"chunk_index\":1}";
        let result = validate_runner_output_payload_json(raw_json);
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
        };

        let raw_json = format!(
            "{{\"stream\":\"{}\",\"encoding\":\"{}\",\"chunk\":\"{}\",\"chunk_index\":{},\"final\":{}}}",
            payload.stream.as_str(),
            payload.encoding.as_str(),
            payload.chunk,
            payload.chunk_index,
            payload.final_chunk,
        );

        let result = validate_runner_output_payload_json(&raw_json);
        assert!(result.is_ok());
    }
}
