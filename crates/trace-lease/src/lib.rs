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
    use super::{GuardError, ReplayState, WorkspaceGuard};

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
}
