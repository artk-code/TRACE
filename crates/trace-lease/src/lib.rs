use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use thiserror::Error;

pub const LEASE_INDEX_DB_PATH: &str = ".trace/leases/index.sqlite3";

#[derive(Debug, Error)]
pub enum LeaseStoreError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
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
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS replay_checkpoint (
                singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                checkpoint_global_seq INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO replay_checkpoint (singleton_id, checkpoint_global_seq)
            VALUES (1, 0);
            ",
        )?;
        Ok(())
    }

    fn open_connection(&self) -> Result<Connection, LeaseStoreError> {
        Ok(Connection::open(&self.db_path)?)
    }
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

    use super::{
        GuardError, ReplayCheckpointStore, ReplayState, WorkspaceGuard, LEASE_INDEX_DB_PATH,
    };

    fn unique_temp_root() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic for test")
            .as_nanos();
        env::temp_dir().join(format!("trace-lease-test-{nanos}"))
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
}
