use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tower_http::trace::TraceLayer;
use trace_api_types::{
    CandidateSummary, OutputEncoding, RunOutputChunk, StatusDetail, Task, TaskResponse, TaskStatus,
    TimelineEvent,
};
use trace_events::{
    validate_runner_output_payload, EventKind, OutputEncoding as EventOutputEncoding,
    OutputStream as EventOutputStream, TraceEvent,
};
use trace_lease::{
    GuardError, LeaseStoreError, ReplayCheckpointStore, ReplayState, WorkspaceGuard,
};
use trace_normalizer::{classify_candidate, filter_candidates, DISQUALIFIED_REASON_STALE_EPOCH};
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

#[derive(Debug, Clone)]
struct TaskProjectionState {
    title: Option<String>,
    owner: Option<String>,
    status: TaskStatus,
    status_detail: Option<StatusDetail>,
    current_epoch: u64,
}

impl Default for TaskProjectionState {
    fn default() -> Self {
        Self {
            title: None,
            owner: None,
            status: TaskStatus::Unclaimed,
            status_detail: None,
            current_epoch: 0,
        }
    }
}

impl TraceApi {
    pub fn from_root(root: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let store = EventStore::new(root);
        Self::from_store(&store)
    }

    pub fn from_store(store: &EventStore) -> Result<Self, std::io::Error> {
        let events = store.read_all_events()?;
        Ok(Self::from_events(events))
    }

    pub fn from_events(mut events: Vec<TraceEvent>) -> Self {
        events.sort_by_key(|event| event.global_seq);

        let mut task_states: HashMap<String, TaskProjectionState> = HashMap::new();
        let mut task_timeline: HashMap<String, Vec<TimelineEvent>> = HashMap::new();
        let mut run_timeline: HashMap<String, Vec<TimelineEvent>> = HashMap::new();
        let mut candidates_by_task: HashMap<String, Vec<CandidateSummary>> = HashMap::new();
        let mut output_by_run: HashMap<String, Vec<RunOutputChunk>> = HashMap::new();

        for event in events {
            let task_id = event.task_id.clone();
            let timeline_event = TimelineEvent {
                kind: event_kind_name(&event.kind),
                ts: event.ts.clone(),
                task_id: task_id.clone(),
                run_id: event.run_id.clone(),
            };

            task_timeline
                .entry(task_id.clone())
                .or_default()
                .push(timeline_event.clone());

            if let Some(run_id) = &event.run_id {
                run_timeline
                    .entry(run_id.clone())
                    .or_default()
                    .push(timeline_event);
            }

            let state = task_states.entry(task_id.clone()).or_default();
            hydrate_task_metadata(state, &event.payload, &task_id);

            match &event.kind {
                EventKind::TaskClaimed | EventKind::TaskRenewed => {
                    apply_claim_event(state, &event.payload);
                }
                EventKind::TaskReleased => {
                    apply_release_event(state, &event.payload);
                }
                EventKind::RunStarted => {
                    apply_run_started_event(state, &event.payload);
                }
                EventKind::ChangesetCreated => {
                    apply_changeset_event(state);

                    if let Some(run_id) = &event.run_id {
                        let candidate = project_candidate(
                            &task_id,
                            run_id,
                            &event.payload,
                            state,
                            event.global_seq,
                        );
                        candidates_by_task
                            .entry(task_id.clone())
                            .or_default()
                            .push(candidate);
                    }
                }
                EventKind::VerdictRecorded => {
                    apply_verdict_event(state, &event.payload);
                }
                EventKind::RunnerOutput => {
                    if let (Some(run_id), Some(output_chunk)) =
                        (&event.run_id, project_output_chunk(&event))
                    {
                        output_by_run
                            .entry(run_id.clone())
                            .or_default()
                            .push(output_chunk);
                    }
                }
                EventKind::Unknown(_) => {}
            }
        }

        let mut tasks = task_states
            .into_iter()
            .map(|(task_id, state)| TaskResponse {
                task: Task {
                    task_id: task_id.clone(),
                    title: state.title.unwrap_or_else(|| format!("Task {task_id}")),
                    owner: state.owner,
                },
                status: state.status,
                status_detail: state.status_detail,
            })
            .collect::<Vec<_>>();

        tasks.sort_by(|left, right| left.task.task_id.cmp(&right.task.task_id));

        for output in output_by_run.values_mut() {
            output.sort_by_key(|chunk| chunk.chunk_index);
        }

        Self {
            tasks,
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

fn event_kind_name(kind: &EventKind) -> String {
    match kind {
        EventKind::TaskClaimed => "task.claimed".to_string(),
        EventKind::TaskRenewed => "task.renewed".to_string(),
        EventKind::TaskReleased => "task.released".to_string(),
        EventKind::VerdictRecorded => "verdict.recorded".to_string(),
        EventKind::RunStarted => "run.started".to_string(),
        EventKind::RunnerOutput => "runner.output".to_string(),
        EventKind::ChangesetCreated => "changeset.created".to_string(),
        EventKind::Unknown(value) => value.clone(),
    }
}

fn hydrate_task_metadata(state: &mut TaskProjectionState, payload: &Value, task_id: &str) {
    if state.title.is_none() {
        state.title = payload_string(payload, &["title", "task_title"])
            .or_else(|| Some(format!("Task {task_id}")));
    }

    if state.owner.is_none() {
        state.owner = payload_string(payload, &["owner", "task_owner"]);
    }
}

fn apply_claim_event(state: &mut TaskProjectionState, payload: &Value) {
    let lease_epoch = payload_u64(payload, &["lease_epoch", "epoch"]).unwrap_or({
        if state.current_epoch == 0 {
            1
        } else {
            state.current_epoch
        }
    });
    state.current_epoch = state.current_epoch.max(lease_epoch);

    state.status = TaskStatus::Claimed;
    state.status_detail = Some(StatusDetail {
        lease_epoch: Some(state.current_epoch),
        holder: payload_string(payload, &["holder", "worker_id", "claimed_by"]),
        reason: payload_string(payload, &["reason"]),
    });
}

fn apply_release_event(state: &mut TaskProjectionState, payload: &Value) {
    state.status = TaskStatus::Unclaimed;
    state.status_detail = Some(StatusDetail {
        lease_epoch: (state.current_epoch > 0).then_some(state.current_epoch),
        holder: None,
        reason: payload_string(payload, &["reason"]),
    });
}

fn apply_run_started_event(state: &mut TaskProjectionState, payload: &Value) {
    state.status = TaskStatus::Running;

    let existing = state.status_detail.clone().unwrap_or(StatusDetail {
        lease_epoch: (state.current_epoch > 0).then_some(state.current_epoch),
        holder: None,
        reason: None,
    });

    let lease_epoch = payload_u64(payload, &["lease_epoch", "epoch"])
        .or(existing.lease_epoch)
        .or((state.current_epoch > 0).then_some(state.current_epoch));

    if let Some(epoch) = lease_epoch {
        state.current_epoch = state.current_epoch.max(epoch);
    }

    state.status_detail = Some(StatusDetail {
        lease_epoch,
        holder: payload_string(payload, &["holder", "worker_id"]).or(existing.holder),
        reason: existing.reason,
    });
}

fn apply_changeset_event(state: &mut TaskProjectionState) {
    state.status = TaskStatus::Evaluating;
}

fn apply_verdict_event(state: &mut TaskProjectionState, payload: &Value) {
    state.status = TaskStatus::Reviewed;

    let existing = state.status_detail.clone().unwrap_or(StatusDetail {
        lease_epoch: (state.current_epoch > 0).then_some(state.current_epoch),
        holder: None,
        reason: None,
    });

    state.status_detail = Some(StatusDetail {
        lease_epoch: existing.lease_epoch,
        holder: existing.holder,
        reason: payload_string(payload, &["reason", "verdict"]).or(existing.reason),
    });
}

fn project_candidate(
    task_id: &str,
    run_id: &str,
    payload: &Value,
    state: &TaskProjectionState,
    global_seq: u64,
) -> CandidateSummary {
    let lease_epoch =
        payload_u64(payload, &["lease_epoch", "epoch"]).unwrap_or(state.current_epoch);
    let current_epoch = state.current_epoch.max(lease_epoch);

    let candidate_id = payload_string(payload, &["candidate_id", "changeset_id", "id"])
        .unwrap_or_else(|| format!("CAND-{global_seq}"));

    let mut candidate = classify_candidate(
        candidate_id,
        task_id.to_string(),
        run_id.to_string(),
        lease_epoch,
        current_epoch,
    );

    if payload_bool(payload, &["stale", "disqualified", "is_stale"]).unwrap_or(false) {
        candidate.eligible = false;
        candidate.disqualified_reason = Some(DISQUALIFIED_REASON_STALE_EPOCH.to_string());
    }

    candidate
}

fn project_output_chunk(event: &TraceEvent) -> Option<RunOutputChunk> {
    let parsed = validate_runner_output_payload(&event.payload).ok()?;

    Some(RunOutputChunk {
        stream: match parsed.stream {
            EventOutputStream::Stdout => "stdout".to_string(),
            EventOutputStream::Stderr => "stderr".to_string(),
        },
        encoding: match parsed.encoding {
            EventOutputEncoding::Utf8 => OutputEncoding::Utf8,
            EventOutputEncoding::Base64 => OutputEncoding::Base64,
        },
        chunk: parsed.chunk,
        chunk_index: parsed.chunk_index,
        final_chunk: parsed.final_chunk,
    })
}

fn payload_value<'a>(payload: &'a Value, key: &str) -> Option<&'a Value> {
    payload
        .get(key)
        .or_else(|| payload.get("task").and_then(|task| task.get(key)))
}

fn payload_string(payload: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = payload_value(payload, key) {
            if let Some(text) = value.as_str() {
                return Some(text.to_string());
            }

            if value.is_number() || value.is_boolean() {
                return Some(value.to_string());
            }
        }
    }

    None
}

fn payload_u64(payload: &Value, keys: &[&str]) -> Option<u64> {
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

fn payload_bool(payload: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(value) = payload_value(payload, key) {
            if let Some(boolean) = value.as_bool() {
                return Some(boolean);
            }

            if let Some(text) = value.as_str() {
                match text {
                    "true" | "TRUE" | "1" => return Some(true),
                    "false" | "FALSE" | "0" => return Some(false),
                    _ => {}
                }
            }
        }
    }

    None
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

    let api = TraceApi::from_store(&event_store)?;

    Ok(ServerRuntime { api, guard })
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
    use std::sync::atomic::{AtomicU64, Ordering};

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use serde_json::{json, Value};
    use tower::util::ServiceExt;
    use trace_events::{EventKind, NewTraceEvent};
    use trace_lease::ReplayCheckpointStore;
    use trace_store::EventStore;

    use super::{app_router, bootstrap_runtime, TraceApi, PHASE0_ENDPOINTS};

    fn unique_temp_root() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be monotonic for test")
            .as_nanos();
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("trace-server-test-{nanos}-{serial}"))
    }

    fn append_event(
        store: &EventStore,
        task_id: &str,
        run_id: Option<&str>,
        kind: EventKind,
        payload: Value,
    ) {
        static EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);
        let tick = EVENT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let sec = 20 + (tick % 30);
        let ts = format!("2026-02-28T05:20:{sec:02}.000Z");

        let event = NewTraceEvent {
            global_seq: None,
            ts,
            task_id: task_id.to_string(),
            run_id: run_id.map(ToString::to_string),
            kind,
            payload,
        };

        store
            .append_event(event)
            .expect("seed event append should succeed");
    }

    fn seed_event_log(root: &std::path::Path) -> EventStore {
        let store = EventStore::new(root);

        append_event(
            &store,
            "TASK-42",
            None,
            EventKind::TaskClaimed,
            json!({
                "epoch": 7,
                "worker_id": "agent-3",
                "title": "Improve lease replay",
                "owner": "platform"
            }),
        );
        append_event(
            &store,
            "TASK-42",
            Some("RUN-13"),
            EventKind::RunStarted,
            json!({}),
        );
        append_event(
            &store,
            "TASK-42",
            Some("RUN-13"),
            EventKind::ChangesetCreated,
            json!({"candidate_id": "C-100", "lease_epoch": 7}),
        );
        append_event(
            &store,
            "TASK-42",
            Some("RUN-12"),
            EventKind::ChangesetCreated,
            json!({"candidate_id": "C-099", "lease_epoch": 6}),
        );
        append_event(
            &store,
            "TASK-42",
            Some("RUN-13"),
            EventKind::RunnerOutput,
            json!({
                "stream": "stdout",
                "encoding": "utf8",
                "chunk": "hello from RUN-13",
                "chunk_index": 0,
                "final": true
            }),
        );

        store
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
        let root = unique_temp_root();
        let store = seed_event_log(&root);

        let api = TraceApi::from_store(&store).expect("projection should build");

        let default_candidates = api.get_task_candidates("TASK-42", false);
        let all_candidates = api.get_task_candidates("TASK-42", true);

        assert_eq!(default_candidates.len(), 1);
        assert_eq!(all_candidates.len(), 2);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_projection_surfaces_output_and_timelines() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);

        let api = TraceApi::from_store(&store).expect("projection should build");

        let task = api.get_task("TASK-42").expect("task should exist");
        assert_eq!(task.task.title, "Improve lease replay");
        assert_eq!(task.status, trace_api_types::TaskStatus::Evaluating);

        assert_eq!(api.get_task_timeline("TASK-42").len(), 5);
        assert_eq!(api.get_run_timeline("RUN-13").len(), 3);
        assert_eq!(api.get_run_output("RUN-13").len(), 1);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_tasks_route_returns_nested_shape() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let api = TraceApi::from_store(&store).expect("projection should build");
        let app = app_router(api);

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

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_candidates_route_honors_query_toggle() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let api = TraceApi::from_store(&store).expect("projection should build");
        let app = app_router(api);

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

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[test]
    fn test_startup_replay_reaches_tip_before_guard() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);

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
