use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
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
        persisted
            .validate()
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;

        let serialized = persisted
            .to_json_line()
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.canonical_log_path)?;

        writeln!(file, "{serialized}")?;

        Ok(persisted)
    }

    pub fn tip_global_seq(&self) -> io::Result<u64> {
        self.current_max_sequence()
    }

    pub fn read_all_events(&self) -> io::Result<Vec<TraceEvent>> {
        if !self.canonical_log_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.canonical_log_path)?;
        let mut events = Vec::new();

        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            let event = TraceEvent::from_json_line(line)
                .map_err(|error| io::Error::new(ErrorKind::InvalidData, error.to_string()))?;
            events.push(event);
        }

        Ok(events)
    }

    fn next_sequence(&self) -> io::Result<u64> {
        let current_max = self.current_max_sequence()?;
        Ok(current_max.saturating_add(1))
    }

    fn current_max_sequence(&self) -> io::Result<u64> {
        if !self.canonical_log_path.exists() {
            return Ok(0);
        }

        let mut max_seq = 0u64;
        for event in self.read_all_events()? {
            if event.global_seq > max_seq {
                max_seq = event.global_seq;
            }
        }

        Ok(max_seq)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;
    use trace_events::{EventKind, NewTraceEvent};

    use super::{EventStore, CANONICAL_EVENT_LOG_PATH};

    fn new_event(task_id: &str) -> NewTraceEvent {
        NewTraceEvent {
            global_seq: None,
            ts: "2026-02-28T05:20:18.123Z".to_string(),
            task_id: task_id.to_string(),
            run_id: Some("RUN-13".to_string()),
            kind: EventKind::RunStarted,
            payload: json!({}),
        }
    }

    fn unique_temp_root() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic for test")
            .as_nanos();
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("trace-store-test-{nanos}-{serial}"))
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
