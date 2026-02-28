#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub task_id: String,
    pub title: String,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Unclaimed,
    Claimed,
    Running,
    Evaluating,
    Reviewed,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusDetail {
    pub lease_epoch: Option<u64>,
    pub holder: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskResponse {
    pub task: Task,
    pub status: TaskStatus,
    pub status_detail: Option<StatusDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEvent {
    pub kind: String,
    pub ts: String,
    pub task_id: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSummary {
    pub candidate_id: String,
    pub task_id: String,
    pub run_id: String,
    pub lease_epoch: u64,
    pub eligible: bool,
    pub disqualified_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputEncoding {
    Utf8,
    Base64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutputChunk {
    pub stream: String,
    pub encoding: OutputEncoding,
    pub chunk: String,
    pub chunk_index: u64,
    pub final_chunk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputDecodeError {
    ExceedsSizeLimit { limit_bytes: usize, attempted_bytes: usize },
    InvalidBase64,
    InvalidUtf8,
}

pub fn validate_task_response_shape(raw_json: &str) -> Result<(), String> {
    if !raw_json.contains("\"status\"") {
        return Err("missing required field: status".to_string());
    }

    if !raw_json.contains("\"task\"") {
        return Err("missing required nested object: task".to_string());
    }

    if raw_json.contains("\"task_id\"") && !raw_json.contains("\"task\":") {
        return Err("flat task_id is not allowed at top level".to_string());
    }

    Ok(())
}

pub fn decode_output_chunk(
    chunk: &RunOutputChunk,
    max_bytes: usize,
) -> Result<String, OutputDecodeError> {
    match chunk.encoding {
        OutputEncoding::Utf8 => {
            if chunk.chunk.len() > max_bytes {
                return Err(OutputDecodeError::ExceedsSizeLimit {
                    limit_bytes: max_bytes,
                    attempted_bytes: chunk.chunk.len(),
                });
            }
            Ok(chunk.chunk.clone())
        }
        OutputEncoding::Base64 => {
            let bytes = decode_base64(chunk.chunk.as_bytes())?;
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

fn decode_base64(input: &[u8]) -> Result<Vec<u8>, OutputDecodeError> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits_in_buffer: u8 = 0;

    for byte in input.iter().copied() {
        if byte == b'=' {
            break;
        }

        let value = base64_value(byte).ok_or(OutputDecodeError::InvalidBase64)? as u32;

        buffer = (buffer << 6) | value;
        bits_in_buffer += 6;

        while bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            let out = ((buffer >> bits_in_buffer) & 0xFF) as u8;
            output.push(out);
        }
    }

    Ok(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
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
        let payload =
            "{\"task\":{\"task_id\":\"TASK-42\",\"title\":\"x\"},\"status\":\"Claimed\"}";
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
