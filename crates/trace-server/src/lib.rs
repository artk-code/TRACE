use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use thiserror::Error;
use tower_http::trace::TraceLayer;
use trace_api_types::{
    CandidateSummary, OutputEncoding, RunOutputChunk, StatusDetail, Task, TaskResponse, TaskStatus,
    TimelineEvent,
};
use trace_lease::{
    GuardError, LeaseStoreError, ReplayCheckpointStore, ReplayState, WorkspaceGuard,
};
use trace_normalizer::{classify_candidate, filter_candidates};
use trace_store::EventStore;

pub const PHASE0_ENDPOINTS: [&str; 6] = [
    "GET /tasks",
    "GET /tasks/:task_id",
    "GET /tasks/:task_id/timeline",
    "GET /runs/:run_id/timeline",
    "GET /tasks/:task_id/candidates?include_disqualified=false",
    "GET /runs/:run_id/output",
];

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("lease store error: {0}")]
    LeaseStore(#[from] LeaseStoreError),
    #[error("replay guard error: {0:?}")]
    Guard(GuardError),
}

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
        task_timeline.insert(
            "TASK-42".to_string(),
            vec![task_event.clone(), run_event.clone()],
        );

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
        self.task_timeline.get(task_id).cloned().unwrap_or_default()
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

#[derive(Debug, Clone)]
pub struct ServerRuntime {
    pub api: TraceApi,
    guard: WorkspaceGuard,
}

impl ServerRuntime {
    pub fn assert_lease_sensitive_ready(&self) -> Result<(), GuardError> {
        self.guard.assert_lease_sensitive_ready()
    }
}

pub fn bootstrap_runtime(root: impl AsRef<Path>) -> Result<ServerRuntime, ServerError> {
    let event_store = EventStore::new(root.as_ref());
    let replay_store = ReplayCheckpointStore::new(root.as_ref())?;

    let tip_global_seq = event_store.tip_global_seq()?;
    replay_store.replay_to_tip(tip_global_seq)?;
    let checkpoint_global_seq = replay_store.checkpoint_global_seq()?;

    let guard = WorkspaceGuard::new(ReplayState {
        checkpoint_global_seq,
        tip_global_seq,
    });

    Ok(ServerRuntime {
        api: TraceApi::sample(),
        guard,
    })
}

#[derive(Clone)]
struct ApiState {
    api: Arc<TraceApi>,
}

#[derive(Debug, Deserialize)]
struct CandidateQuery {
    include_disqualified: Option<bool>,
}

pub fn app_router(api: TraceApi) -> Router {
    let state = ApiState { api: Arc::new(api) };

    Router::new()
        .route("/tasks", get(get_tasks_handler))
        .route("/tasks/{task_id}", get(get_task_handler))
        .route("/tasks/{task_id}/timeline", get(get_task_timeline_handler))
        .route("/runs/{run_id}/timeline", get(get_run_timeline_handler))
        .route(
            "/tasks/{task_id}/candidates",
            get(get_task_candidates_handler),
        )
        .route("/runs/{run_id}/output", get(get_run_output_handler))
        .with_state(state)
}

pub async fn serve(addr: SocketAddr, root: impl AsRef<Path>) -> Result<(), ServerError> {
    let runtime = bootstrap_runtime(root)?;
    runtime
        .assert_lease_sensitive_ready()
        .map_err(ServerError::Guard)?;

    let app = app_router(runtime.api).layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_tasks_handler(State(state): State<ApiState>) -> Json<Vec<TaskResponse>> {
    Json(state.api.get_tasks())
}

async fn get_task_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<TaskResponse>, StatusCode> {
    state
        .api
        .get_task(&task_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_task_timeline_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
) -> Json<Vec<TimelineEvent>> {
    Json(state.api.get_task_timeline(&task_id))
}

async fn get_run_timeline_handler(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
) -> Json<Vec<TimelineEvent>> {
    Json(state.api.get_run_timeline(&run_id))
}

async fn get_task_candidates_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
    Query(query): Query<CandidateQuery>,
) -> Json<Vec<CandidateSummary>> {
    let include_disqualified = query.include_disqualified.unwrap_or(false);
    Json(
        state
            .api
            .get_task_candidates(&task_id, include_disqualified),
    )
}

async fn get_run_output_handler(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
) -> Json<Vec<RunOutputChunk>> {
    Json(state.api.get_run_output(&run_id))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use serde_json::{json, Value};
    use tower::util::ServiceExt;
    use trace_events::{EventKind, NewTraceEvent};
    use trace_lease::ReplayCheckpointStore;
    use trace_store::EventStore;

    use super::{app_router, bootstrap_runtime, TraceApi, PHASE0_ENDPOINTS};

    fn unique_temp_root() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic for test")
            .as_nanos();
        env::temp_dir().join(format!("trace-server-test-{nanos}"))
    }

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

    #[tokio::test]
    async fn test_tasks_route_returns_nested_shape() {
        let app = app_router(TraceApi::sample());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should be JSON");

        assert!(parsed[0].get("task").is_some());
        assert!(parsed[0].get("task_id").is_none());
    }

    #[tokio::test]
    async fn test_candidates_route_honors_query_toggle() {
        let app = app_router(TraceApi::sample());

        let default_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-42/candidates")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let default_body = to_bytes(default_response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let default_candidates: Vec<Value> =
            serde_json::from_slice(&default_body).expect("response should parse");

        let full_response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-42/candidates?include_disqualified=true")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let full_body = to_bytes(full_response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let all_candidates: Vec<Value> =
            serde_json::from_slice(&full_body).expect("response should parse");

        assert_eq!(default_candidates.len(), 1);
        assert_eq!(all_candidates.len(), 2);
    }

    #[test]
    fn test_startup_replay_reaches_tip_before_guard() {
        let root = unique_temp_root();
        let store = EventStore::new(&root);

        let event = NewTraceEvent {
            global_seq: None,
            ts: "2026-02-28T05:21:01.000Z".to_string(),
            task_id: "TASK-42".to_string(),
            run_id: Some("RUN-13".to_string()),
            kind: EventKind::RunStarted,
            payload: json!({}),
        };

        store
            .append_event(event)
            .expect("event should be appended before startup");

        let runtime = bootstrap_runtime(&root).expect("runtime bootstrap should succeed");
        runtime
            .assert_lease_sensitive_ready()
            .expect("guard should be open after replay");

        let replay_store = ReplayCheckpointStore::new(&root).expect("checkpoint store should open");
        assert_eq!(
            replay_store
                .checkpoint_global_seq()
                .expect("checkpoint should exist"),
            store.tip_global_seq().expect("tip should read")
        );

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }
}
