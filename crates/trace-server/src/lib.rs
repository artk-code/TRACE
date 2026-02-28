use std::collections::HashMap;

use trace_api_types::{
    CandidateSummary, OutputEncoding, RunOutputChunk, StatusDetail, Task, TaskResponse, TaskStatus,
    TimelineEvent,
};
use trace_normalizer::{classify_candidate, filter_candidates};

pub const PHASE0_ENDPOINTS: [&str; 6] = [
    "GET /tasks",
    "GET /tasks/:task_id",
    "GET /tasks/:task_id/timeline",
    "GET /runs/:run_id/timeline",
    "GET /tasks/:task_id/candidates?include_disqualified=false",
    "GET /runs/:run_id/output",
];

#[derive(Debug, Clone)]
pub struct TraceApi {
    tasks: Vec<TaskResponse>,
    task_timeline: HashMap<String, Vec<TimelineEvent>>,
    run_timeline: HashMap<String, Vec<TimelineEvent>>,
    candidates_by_task: HashMap<String, Vec<CandidateSummary>>,
    output_by_run: HashMap<String, Vec<RunOutputChunk>>,
}

impl TraceApi {
    pub fn sample() -> Self {
        let task = TaskResponse {
            task: Task {
                task_id: "TASK-42".to_string(),
                title: "Improve lease replay".to_string(),
                owner: Some("platform".to_string()),
            },
            status: TaskStatus::Claimed,
            status_detail: Some(StatusDetail {
                lease_epoch: Some(7),
                holder: Some("agent-3".to_string()),
                reason: None,
            }),
        };

        let task_event = TimelineEvent {
            kind: "task.claimed".to_string(),
            ts: "2026-02-28T05:20:18.123Z".to_string(),
            task_id: "TASK-42".to_string(),
            run_id: None,
        };

        let run_event = TimelineEvent {
            kind: "run.started".to_string(),
            ts: "2026-02-28T05:21:01.000Z".to_string(),
            task_id: "TASK-42".to_string(),
            run_id: Some("RUN-13".to_string()),
        };

        let candidates = vec![
            classify_candidate("C-100", "TASK-42", "RUN-13", 7, 7),
            classify_candidate("C-099", "TASK-42", "RUN-12", 6, 7),
        ];

        let output_chunks = vec![RunOutputChunk {
            stream: "stdout".to_string(),
            encoding: OutputEncoding::Utf8,
            chunk: "hello from RUN-13".to_string(),
            chunk_index: 0,
            final_chunk: true,
        }];

        let mut task_timeline = HashMap::new();
        task_timeline.insert("TASK-42".to_string(), vec![task_event.clone(), run_event.clone()]);

        let mut run_timeline = HashMap::new();
        run_timeline.insert("RUN-13".to_string(), vec![run_event]);

        let mut candidates_by_task = HashMap::new();
        candidates_by_task.insert("TASK-42".to_string(), candidates);

        let mut output_by_run = HashMap::new();
        output_by_run.insert("RUN-13".to_string(), output_chunks);

        Self {
            tasks: vec![task],
            task_timeline,
            run_timeline,
            candidates_by_task,
            output_by_run,
        }
    }

    pub fn get_tasks(&self) -> Vec<TaskResponse> {
        self.tasks.clone()
    }

    pub fn get_task(&self, task_id: &str) -> Option<TaskResponse> {
        self.tasks
            .iter()
            .find(|task| task.task.task_id == task_id)
            .cloned()
    }

    pub fn get_task_timeline(&self, task_id: &str) -> Vec<TimelineEvent> {
        self.task_timeline
            .get(task_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_run_timeline(&self, run_id: &str) -> Vec<TimelineEvent> {
        self.run_timeline.get(run_id).cloned().unwrap_or_default()
    }

    pub fn get_task_candidates(
        &self,
        task_id: &str,
        include_disqualified: bool,
    ) -> Vec<CandidateSummary> {
        self.candidates_by_task
            .get(task_id)
            .map(|candidates| filter_candidates(candidates, include_disqualified))
            .unwrap_or_default()
    }

    pub fn get_run_output(&self, run_id: &str) -> Vec<RunOutputChunk> {
        self.output_by_run.get(run_id).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{TraceApi, PHASE0_ENDPOINTS};

    #[test]
    fn test_phase0_endpoint_set_is_locked() {
        assert_eq!(PHASE0_ENDPOINTS.len(), 6);
        assert_eq!(PHASE0_ENDPOINTS[0], "GET /tasks");
        assert_eq!(
            PHASE0_ENDPOINTS[4],
            "GET /tasks/:task_id/candidates?include_disqualified=false"
        );
    }

    #[test]
    fn test_candidates_exclude_disqualified_by_default() {
        let api = TraceApi::sample();

        let default_candidates = api.get_task_candidates("TASK-42", false);
        let all_candidates = api.get_task_candidates("TASK-42", true);

        assert_eq!(default_candidates.len(), 1);
        assert_eq!(all_candidates.len(), 2);
    }
}
