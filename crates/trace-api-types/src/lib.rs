use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub task_id: String,
    pub title: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Unclaimed,
    Claimed,
    Running,
    Evaluating,
    Reviewed,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusDetail {
    pub lease_epoch: Option<u64>,
    pub holder: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResponse {
    pub task: Task,
    pub status: TaskStatus,
    pub status_detail: Option<StatusDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineEvent {
    pub kind: String,
    pub ts: String,
    pub task_id: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSummary {
    pub candidate_id: String,
    pub task_id: String,
    pub run_id: String,
    pub lease_epoch: u64,
    pub eligible: bool,
    pub disqualified_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputEncoding {
    Utf8,
    Base64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOutputChunk {
    pub stream: String,
    pub encoding: OutputEncoding,
    pub chunk: String,
    pub chunk_index: u64,
    #[serde(rename = "final", default)]
    pub final_chunk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDecodeError {
    ExceedsSizeLimit {
        limit_bytes: usize,
        attempted_bytes: usize,
    },
    InvalidBase64,
    InvalidUtf8,
}

pub fn validate_task_response_shape(raw_json: &str) -> Result<(), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw_json).map_err(|error| error.to_string())?;

    if parsed.get("task_id").is_some() {
        return Err("flat task_id is not allowed at top level".to_string());
    }

    serde_json::from_value::<TaskResponse>(parsed)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn decode_output_chunk(
    chunk: &RunOutputChunk,
    max_bytes: usize,
) -> Result<String, OutputDecodeError> {
    match chunk.encoding {
        OutputEncoding::Utf8 => {
            let byte_len = chunk.chunk.len();
            if byte_len > max_bytes {
                return Err(OutputDecodeError::ExceedsSizeLimit {
                    limit_bytes: max_bytes,
                    attempted_bytes: byte_len,
                });
            }
            Ok(chunk.chunk.clone())
        }
        OutputEncoding::Base64 => {
            let bytes = BASE64_STANDARD
                .decode(chunk.chunk.as_bytes())
                .map_err(|_| OutputDecodeError::InvalidBase64)?;
            if bytes.len() > max_bytes {
                return Err(OutputDecodeError::ExceedsSizeLimit {
                    limit_bytes: max_bytes,
                    attempted_bytes: bytes.len(),
                });
            }

            String::from_utf8(bytes).map_err(|_| OutputDecodeError::InvalidUtf8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_output_chunk, validate_task_response_shape, OutputDecodeError, OutputEncoding,
        RunOutputChunk,
    };

    #[test]
    fn test_task_response_accepts_nested_shape() {
        let payload = "{\"task\":{\"task_id\":\"TASK-42\",\"title\":\"x\"},\"status\":\"Claimed\"}";
        assert!(validate_task_response_shape(payload).is_ok());
    }

    #[test]
    fn test_task_response_rejects_flat_shape() {
        let payload = "{\"task_id\":\"TASK-42\",\"title\":\"x\",\"status\":\"Claimed\"}";
        assert!(validate_task_response_shape(payload).is_err());
    }

    #[test]
    fn test_runner_output_base64_chunk_decode_with_size_guard() {
        let chunk = RunOutputChunk {
            stream: "stdout".to_string(),
            encoding: OutputEncoding::Base64,
            chunk: "aGVsbG8=".to_string(),
            chunk_index: 0,
            final_chunk: true,
        };

        let decoded = decode_output_chunk(&chunk, 1024).expect("chunk should decode");
        assert_eq!(decoded, "hello");

        let too_small = decode_output_chunk(&chunk, 3);
        assert_eq!(
            too_small,
            Err(OutputDecodeError::ExceedsSizeLimit {
                limit_bytes: 3,
                attempted_bytes: 5,
            })
        );
    }
}
