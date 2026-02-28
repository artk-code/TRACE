use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use trace_events::{EventKind, TraceEvent};

pub const LEASE_INDEX_DB_PATH: &str = ".trace/leases/index.sqlite3";

#[derive(Debug, Error)]
pub enum LeaseStoreError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseState {
    pub task_id: String,
    pub holder: Option<String>,
    pub lease_epoch: u64,
    pub active: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LeaseApplyError {
    #[error("stale_epoch: provided={provided_epoch}, current={current_epoch}")]
    StaleEpoch {
        provided_epoch: u64,
        current_epoch: u64,
    },
    #[error("lease already claimed by {holder} at epoch {lease_epoch}")]
    AlreadyClaimed { holder: String, lease_epoch: u64 },
    #[error("lease is not currently claimed")]
    LeaseNotClaimed,
    #[error("lease holder mismatch: expected={expected_holder}, provided={provided_holder}")]
    HolderMismatch {
        expected_holder: String,
        provided_holder: String,
    },
}

#[derive(Debug, Clone)]
pub struct ReplayCheckpointStore {
    db_path: PathBuf,
}

impl ReplayCheckpointStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, LeaseStoreError> {
        let db_path = root.as_ref().join(LEASE_INDEX_DB_PATH);
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let store = Self { db_path };
        store.initialize()?;
        Ok(store)
    }

    pub fn checkpoint_global_seq(&self) -> Result<u64, LeaseStoreError> {
        let conn = self.open_connection()?;
        let value: u64 = conn.query_row(
            "SELECT checkpoint_global_seq FROM replay_checkpoint WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(value)
    }

    pub fn set_checkpoint_global_seq(
        &self,
        checkpoint_global_seq: u64,
    ) -> Result<(), LeaseStoreError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE replay_checkpoint SET checkpoint_global_seq = ?1 WHERE singleton_id = 1",
            params![checkpoint_global_seq],
        )?;
        Ok(())
    }

    pub fn replay_to_tip(&self, tip_global_seq: u64) -> Result<(), LeaseStoreError> {
        self.set_checkpoint_global_seq(tip_global_seq)
    }

    fn initialize(&self) -> Result<(), LeaseStoreError> {
        let conn = self.open_connection()?;
        initialize_schema(&conn)?;
        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, LeaseStoreError> {
        Ok(Connection::open(&self.db_path)?)
    }
}

#[derive(Debug, Clone)]
pub struct LeaseIndexStore {
    db_path: PathBuf,
}

impl LeaseIndexStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, LeaseStoreError> {
        let db_path = root.as_ref().join(LEASE_INDEX_DB_PATH);
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let store = Self { db_path };
        store.initialize()?;
        Ok(store)
    }

    pub fn current_lease(&self, task_id: &str) -> Result<Option<LeaseState>, LeaseStoreError> {
        let conn = self.open_connection()?;
        let lease = conn
            .query_row(
                "SELECT holder, lease_epoch, active FROM lease_state WHERE task_id = ?1",
                params![task_id],
                |row| {
                    let holder: Option<String> = row.get(0)?;
                    let lease_epoch: u64 = row.get(1)?;
                    let active: i64 = row.get(2)?;
                    Ok(LeaseState {
                        task_id: task_id.to_string(),
                        holder,
                        lease_epoch,
                        active: active == 1,
                    })
                },
            )
            .optional()?;

        Ok(lease)
    }

    pub fn apply_claim(
        &self,
        task_id: &str,
        worker_id: &str,
        expected_epoch: Option<u64>,
    ) -> Result<LeaseState, LeaseApplyError> {
        let current = self
            .current_lease(task_id)
            .map_err(|_| LeaseApplyError::LeaseNotClaimed)?;

        let current_epoch = current.as_ref().map(|lease| lease.lease_epoch).unwrap_or(0);
        if let Some(provided_epoch) = expected_epoch {
            if provided_epoch < current_epoch {
                return Err(LeaseApplyError::StaleEpoch {
                    provided_epoch,
                    current_epoch,
                });
            }
        }

        if let Some(lease) = current {
            if lease.active {
                return Err(LeaseApplyError::AlreadyClaimed {
                    holder: lease.holder.unwrap_or_else(|| "unknown".to_string()),
                    lease_epoch: lease.lease_epoch,
                });
            }
        }

        let updated = LeaseState {
            task_id: task_id.to_string(),
            holder: Some(worker_id.to_string()),
            lease_epoch: current_epoch + 1,
            active: true,
        };

        self.upsert_lease(&updated)
            .map_err(|_| LeaseApplyError::LeaseNotClaimed)?;
        Ok(updated)
    }

    pub fn apply_renew(
        &self,
        task_id: &str,
        worker_id: &str,
        lease_epoch: u64,
    ) -> Result<LeaseState, LeaseApplyError> {
        let current = self
            .current_lease(task_id)
            .map_err(|_| LeaseApplyError::LeaseNotClaimed)?
            .ok_or(LeaseApplyError::LeaseNotClaimed)?;

        if !current.active {
            return Err(LeaseApplyError::LeaseNotClaimed);
        }

        let expected_holder = current.holder.unwrap_or_else(|| "unknown".to_string());
        if expected_holder != worker_id {
            return Err(LeaseApplyError::HolderMismatch {
                expected_holder,
                provided_holder: worker_id.to_string(),
            });
        }

        if lease_epoch != current.lease_epoch {
            return Err(LeaseApplyError::StaleEpoch {
                provided_epoch: lease_epoch,
                current_epoch: current.lease_epoch,
            });
        }

        let updated = LeaseState {
            task_id: task_id.to_string(),
            holder: Some(worker_id.to_string()),
            lease_epoch,
            active: true,
        };

        self.upsert_lease(&updated)
            .map_err(|_| LeaseApplyError::LeaseNotClaimed)?;
        Ok(updated)
    }

    pub fn apply_release(
        &self,
        task_id: &str,
        worker_id: &str,
        lease_epoch: u64,
    ) -> Result<LeaseState, LeaseApplyError> {
        let current = self
            .current_lease(task_id)
            .map_err(|_| LeaseApplyError::LeaseNotClaimed)?
            .ok_or(LeaseApplyError::LeaseNotClaimed)?;

        if !current.active {
            return Err(LeaseApplyError::LeaseNotClaimed);
        }

        let expected_holder = current.holder.unwrap_or_else(|| "unknown".to_string());
        if expected_holder != worker_id {
            return Err(LeaseApplyError::HolderMismatch {
                expected_holder,
                provided_holder: worker_id.to_string(),
            });
        }

        if lease_epoch != current.lease_epoch {
            return Err(LeaseApplyError::StaleEpoch {
                provided_epoch: lease_epoch,
                current_epoch: current.lease_epoch,
            });
        }

        let updated = LeaseState {
            task_id: task_id.to_string(),
            holder: None,
            lease_epoch,
            active: false,
        };

        self.upsert_lease(&updated)
            .map_err(|_| LeaseApplyError::LeaseNotClaimed)?;
        Ok(updated)
    }

    pub fn apply_event(&self, event: &TraceEvent) -> Result<(), LeaseStoreError> {
        match &event.kind {
            EventKind::TaskClaimed => {
                let current = self.current_lease(&event.task_id)?;
                let fallback_epoch = current
                    .as_ref()
                    .map(|lease| lease.lease_epoch + 1)
                    .unwrap_or(1);
                let lease_epoch = payload_u64(&event.payload, &["lease_epoch", "epoch"])
                    .unwrap_or(fallback_epoch);
                let holder = payload_string(&event.payload, &["worker_id", "holder", "claimed_by"]);

                self.upsert_lease(&LeaseState {
                    task_id: event.task_id.clone(),
                    holder,
                    lease_epoch,
                    active: true,
                })?;
            }
            EventKind::TaskRenewed => {
                let current = self.current_lease(&event.task_id)?;
                let fallback_epoch = current.as_ref().map(|lease| lease.lease_epoch).unwrap_or(1);
                let lease_epoch = payload_u64(&event.payload, &["lease_epoch", "epoch"])
                    .unwrap_or(fallback_epoch);
                let holder = payload_string(&event.payload, &["worker_id", "holder", "claimed_by"])
                    .or_else(|| current.and_then(|lease| lease.holder));

                self.upsert_lease(&LeaseState {
                    task_id: event.task_id.clone(),
                    holder,
                    lease_epoch,
                    active: true,
                })?;
            }
            EventKind::TaskReleased => {
                let current = self.current_lease(&event.task_id)?;
                let fallback_epoch = current.as_ref().map(|lease| lease.lease_epoch).unwrap_or(0);
                let lease_epoch = payload_u64(&event.payload, &["lease_epoch", "epoch"])
                    .unwrap_or(fallback_epoch);

                self.upsert_lease(&LeaseState {
                    task_id: event.task_id.clone(),
                    holder: None,
                    lease_epoch,
                    active: false,
                })?;
            }
            _ => {}
        }

        Ok(())
    }

    pub fn replay_events(&self, events: &[TraceEvent]) -> Result<(), LeaseStoreError> {
        self.clear_leases()?;

        let mut ordered = events.to_vec();
        ordered.sort_by_key(|event| event.global_seq);
        for event in &ordered {
            self.apply_event(event)?;
        }

        Ok(())
    }

    fn clear_leases(&self) -> Result<(), LeaseStoreError> {
        let conn = self.open_connection()?;
        conn.execute("DELETE FROM lease_state", [])?;
        Ok(())
    }

    fn initialize(&self) -> Result<(), LeaseStoreError> {
        let conn = self.open_connection()?;
        initialize_schema(&conn)?;
        Ok(())
    }

    fn upsert_lease(&self, lease: &LeaseState) -> Result<(), LeaseStoreError> {
        let conn = self.open_connection()?;
        conn.execute(
            "
            INSERT INTO lease_state (task_id, holder, lease_epoch, active)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(task_id) DO UPDATE SET
                holder = excluded.holder,
                lease_epoch = excluded.lease_epoch,
                active = excluded.active
            ",
            params![
                lease.task_id,
                lease.holder,
                lease.lease_epoch,
                i64::from(lease.active)
            ],
        )?;

        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, LeaseStoreError> {
        Ok(Connection::open(&self.db_path)?)
    }
}

fn initialize_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS replay_checkpoint (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            checkpoint_global_seq INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO replay_checkpoint (singleton_id, checkpoint_global_seq)
        VALUES (1, 0);

        CREATE TABLE IF NOT EXISTS lease_state (
            task_id TEXT PRIMARY KEY,
            holder TEXT,
            lease_epoch INTEGER NOT NULL,
            active INTEGER NOT NULL CHECK (active IN (0, 1))
        );
        ",
    )
}

fn payload_value<'a>(payload: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    payload
        .get(key)
        .or_else(|| payload.get("task").and_then(|task| task.get(key)))
}

fn payload_string(payload: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = payload_value(payload, key) {
            if let Some(text) = value.as_str() {
                return Some(text.to_string());
            }
        }
    }

    None
}

fn payload_u64(payload: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(value) = payload_value(payload, key) {
            if let Some(number) = value.as_u64() {
                return Some(number);
            }
            if let Some(text) = value.as_str() {
                if let Ok(number) = text.parse::<u64>() {
                    return Some(number);
                }
            }
        }
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayState {
    pub checkpoint_global_seq: u64,
    pub tip_global_seq: u64,
}

impl ReplayState {
    pub fn is_caught_up(self) -> bool {
        self.checkpoint_global_seq >= self.tip_global_seq
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardError {
    ReplayBehind {
        checkpoint_global_seq: u64,
        tip_global_seq: u64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct WorkspaceGuard {
    state: ReplayState,
}

impl WorkspaceGuard {
    pub fn new(state: ReplayState) -> Self {
        Self { state }
    }

    pub fn set_checkpoint_global_seq(&mut self, checkpoint_global_seq: u64) {
        self.state.checkpoint_global_seq = checkpoint_global_seq;
    }

    pub fn assert_lease_sensitive_ready(&self) -> Result<(), GuardError> {
        if self.state.is_caught_up() {
            Ok(())
        } else {
            Err(GuardError::ReplayBehind {
                checkpoint_global_seq: self.state.checkpoint_global_seq,
                tip_global_seq: self.state.tip_global_seq,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;
    use trace_events::{EventKind, TraceEvent};

    use super::{
        GuardError, LeaseApplyError, LeaseIndexStore, ReplayCheckpointStore, ReplayState,
        WorkspaceGuard, LEASE_INDEX_DB_PATH,
    };

    fn unique_temp_root() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic for test")
            .as_nanos();
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("trace-lease-test-{nanos}-{serial}"))
    }

    #[test]
    fn test_replay_gate_blocks_claim_when_checkpoint_behind() {
        let guard = WorkspaceGuard::new(ReplayState {
            checkpoint_global_seq: 5,
            tip_global_seq: 6,
        });

        let error = guard
            .assert_lease_sensitive_ready()
            .expect_err("guard should reject claim path while replay is behind");

        assert_eq!(
            error,
            GuardError::ReplayBehind {
                checkpoint_global_seq: 5,
                tip_global_seq: 6,
            }
        );
    }

    #[test]
    fn test_replay_gate_enables_claim_after_tip_reached() {
        let mut guard = WorkspaceGuard::new(ReplayState {
            checkpoint_global_seq: 5,
            tip_global_seq: 6,
        });

        guard.set_checkpoint_global_seq(6);

        let result = guard.assert_lease_sensitive_ready();
        assert!(result.is_ok());
    }

    #[test]
    fn test_checkpoint_store_defaults_to_zero() {
        let root = unique_temp_root();
        let store = ReplayCheckpointStore::new(&root).expect("store should initialize");

        assert_eq!(store.checkpoint_global_seq().unwrap_or_default(), 0);
        assert!(root.join(LEASE_INDEX_DB_PATH).exists());

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_replay_to_tip_updates_checkpoint() {
        let root = unique_temp_root();
        let store = ReplayCheckpointStore::new(&root).expect("store should initialize");

        store
            .replay_to_tip(9)
            .expect("replay should update checkpoint");

        assert_eq!(store.checkpoint_global_seq().unwrap_or_default(), 9);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_apply_claim_renew_release_roundtrip() {
        let root = unique_temp_root();
        let store = LeaseIndexStore::new(&root).expect("lease store should initialize");

        let claim = store
            .apply_claim("TASK-1", "agent-1", None)
            .expect("claim should succeed");
        assert!(claim.active);
        assert_eq!(claim.lease_epoch, 1);

        let renewed = store
            .apply_renew("TASK-1", "agent-1", 1)
            .expect("renew should succeed");
        assert!(renewed.active);
        assert_eq!(renewed.lease_epoch, 1);

        let released = store
            .apply_release("TASK-1", "agent-1", 1)
            .expect("release should succeed");
        assert!(!released.active);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_apply_claim_rejects_stale_epoch() {
        let root = unique_temp_root();
        let store = LeaseIndexStore::new(&root).expect("lease store should initialize");

        store
            .apply_claim("TASK-1", "agent-1", None)
            .expect("initial claim should succeed");
        store
            .apply_release("TASK-1", "agent-1", 1)
            .expect("release should succeed");

        let error = store
            .apply_claim("TASK-1", "agent-2", Some(0))
            .expect_err("stale expected epoch should fail");

        assert_eq!(
            error,
            LeaseApplyError::StaleEpoch {
                provided_epoch: 0,
                current_epoch: 1,
            }
        );

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_replay_events_restores_latest_lease_state() {
        let root = unique_temp_root();
        let store = LeaseIndexStore::new(&root).expect("lease store should initialize");

        let events = vec![
            TraceEvent {
                global_seq: 1,
                ts: "2026-02-28T01:00:00.000Z".to_string(),
                task_id: "TASK-7".to_string(),
                run_id: None,
                kind: EventKind::TaskClaimed,
                payload: json!({"epoch": 4, "worker_id": "agent-a"}),
            },
            TraceEvent {
                global_seq: 2,
                ts: "2026-02-28T01:01:00.000Z".to_string(),
                task_id: "TASK-7".to_string(),
                run_id: None,
                kind: EventKind::TaskReleased,
                payload: json!({"epoch": 4}),
            },
        ];

        store
            .replay_events(&events)
            .expect("replay should rebuild lease index");

        let state = store
            .current_lease("TASK-7")
            .expect("query should succeed")
            .expect("lease state should exist");
        assert_eq!(state.lease_epoch, 4);
        assert!(!state.active);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }
}
