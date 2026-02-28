use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use trace_events::{NewTraceEvent, TraceEvent};

pub const CANONICAL_EVENT_LOG_PATH: &str = ".trace/events/events.jsonl";

#[derive(Debug, Clone)]
pub struct EventStore {
    canonical_log_path: PathBuf,
}

impl EventStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            canonical_log_path: root.as_ref().join(CANONICAL_EVENT_LOG_PATH),
        }
    }

    pub fn canonical_log_path(&self) -> &Path {
        &self.canonical_log_path
    }

    pub fn append_event(&self, event: NewTraceEvent) -> io::Result<TraceEvent> {
        if event.global_seq.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "new events must not include global_seq before persist",
            ));
        }

        if let Some(parent) = self.canonical_log_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let next_seq = self.next_sequence()?;
        let persisted = event.persist_with_global_seq(next_seq);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.canonical_log_path)?;

        writeln!(file, "{}", persisted.to_json_line())?;

        Ok(persisted)
    }

    pub fn tip_global_seq(&self) -> io::Result<u64> {
        self.current_max_sequence()
    }

    fn next_sequence(&self) -> io::Result<u64> {
        let current_max = self.current_max_sequence()?;
        Ok(current_max.saturating_add(1))
    }

    fn current_max_sequence(&self) -> io::Result<u64> {
        if !self.canonical_log_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&self.canonical_log_path)?;
        let mut max_seq = 0u64;

        for line in content.lines() {
            if let Some(seq) = extract_global_seq(line) {
                if seq > max_seq {
                    max_seq = seq;
                }
            }
        }

        Ok(max_seq)
    }
}

pub fn extract_global_seq(line: &str) -> Option<u64> {
    let needle = "\"global_seq\":";
    let start = line.find(needle)? + needle.len();

    let digits: String = line[start..]
        .chars()
        .skip_while(|ch| ch.is_ascii_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        return None;
    }

    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;

    use trace_events::{EventKind, EventPayload, NewTraceEvent};

    use super::{CANONICAL_EVENT_LOG_PATH, EventStore};

    fn new_event(task_id: &str) -> NewTraceEvent {
        NewTraceEvent {
            global_seq: None,
            ts: "2026-02-28T05:20:18.123Z".to_string(),
            task_id: task_id.to_string(),
            run_id: Some("RUN-13".to_string()),
            kind: EventKind::RunStarted,
            payload: EventPayload::RawObject("{}".to_string()),
        }
    }

    fn unique_temp_root() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic for test")
            .as_nanos();
        env::temp_dir().join(format!("trace-store-test-{nanos}"))
    }

    #[test]
    fn test_event_log_path_is_canonical() {
        assert_eq!(CANONICAL_EVENT_LOG_PATH, ".trace/events/events.jsonl");
    }

    #[test]
    fn test_trace_event_requires_global_seq_on_persisted_reads() {
        let root = unique_temp_root();
        let store = EventStore::new(&root);

        let persisted = store
            .append_event(new_event("TASK-1"))
            .expect("append should work");

        assert!(persisted.global_seq > 0);
        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_assigns_monotonic_global_seq() {
        let root = unique_temp_root();
        let store = EventStore::new(&root);

        let first = store
            .append_event(new_event("TASK-1"))
            .expect("first append should work");
        let second = store
            .append_event(new_event("TASK-2"))
            .expect("second append should work");

        assert_eq!(first.global_seq, 1);
        assert_eq!(second.global_seq, 2);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }
}
