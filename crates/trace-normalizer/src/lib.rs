use trace_api_types::CandidateSummary;

pub const DISQUALIFIED_REASON_STALE_EPOCH: &str = "stale_epoch";

pub fn classify_candidate(
    candidate_id: impl Into<String>,
    task_id: impl Into<String>,
    run_id: impl Into<String>,
    lease_epoch: u64,
    current_epoch: u64,
) -> CandidateSummary {
    let stale = lease_epoch < current_epoch;

    CandidateSummary {
        candidate_id: candidate_id.into(),
        task_id: task_id.into(),
        run_id: run_id.into(),
        lease_epoch,
        eligible: !stale,
        disqualified_reason: stale.then(|| DISQUALIFIED_REASON_STALE_EPOCH.to_string()),
    }
}

pub fn filter_candidates(
    candidates: &[CandidateSummary],
    include_disqualified: bool,
) -> Vec<CandidateSummary> {
    if include_disqualified {
        candidates.to_vec()
    } else {
        candidates
            .iter()
            .filter(|candidate| candidate.eligible)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_candidate, filter_candidates, DISQUALIFIED_REASON_STALE_EPOCH,
    };

    #[test]
    fn test_stale_epoch_candidate_marked_disqualified() {
        let candidate = classify_candidate("C1", "TASK-1", "RUN-1", 6, 7);

        assert!(!candidate.eligible);
        assert_eq!(
            candidate.disqualified_reason.as_deref(),
            Some(DISQUALIFIED_REASON_STALE_EPOCH)
        );
    }

    #[test]
    fn test_candidate_views_exclude_disqualified_by_default() {
        let candidates = vec![
            classify_candidate("C1", "TASK-1", "RUN-1", 7, 7),
            classify_candidate("C2", "TASK-1", "RUN-2", 6, 7),
        ];

        let visible = filter_candidates(&candidates, false);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].candidate_id, "C1");
    }
}
