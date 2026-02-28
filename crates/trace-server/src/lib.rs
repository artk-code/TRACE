use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;
use trace_api_types::{
    CandidateSummary, OutputEncoding, RunOutputChunk, StatusDetail, Task, TaskResponse, TaskStatus,
    TimelineEvent,
};
use trace_events::{
    validate_runner_output_payload, EventKind, NewTraceEvent,
    OutputEncoding as EventOutputEncoding, OutputStream as EventOutputStream, TraceEvent,
};
use trace_lease::{
    GuardError, LeaseIndexStore, LeaseStoreError, ReplayCheckpointStore, ReplayState,
    WorkspaceGuard,
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

const DEFAULT_CORS_ALLOWED_ORIGINS: [&str; 4] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "http://localhost:4173",
    "http://127.0.0.1:4173",
];
const DEFAULT_TMUX_SESSION: &str = "trace-smoke";
const DEFAULT_TMUX_SCRIPT_PATH: &str = "scripts/trace-smoke-tmux.sh";

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
    event_store: EventStore,
    lease_store: LeaseIndexStore,
    replay_store: ReplayCheckpointStore,
    guard: WorkspaceGuard,
}

impl ServerRuntime {
    pub fn assert_lease_sensitive_ready(&self) -> Result<(), GuardError> {
        self.guard.assert_lease_sensitive_ready()
    }
}

pub fn bootstrap_runtime(root: impl AsRef<Path>) -> Result<ServerRuntime, ServerError> {
    let event_store = EventStore::new(root.as_ref());
    let lease_store = LeaseIndexStore::new(root.as_ref())?;
    let replay_store = ReplayCheckpointStore::new(root.as_ref())?;

    let events = event_store.read_all_events()?;
    lease_store.replay_events(&events)?;

    let tip_global_seq = events.last().map(|event| event.global_seq).unwrap_or(0);
    replay_store.replay_to_tip(tip_global_seq)?;
    let checkpoint_global_seq = replay_store.checkpoint_global_seq()?;

    let guard = WorkspaceGuard::new(ReplayState {
        checkpoint_global_seq,
        tip_global_seq,
    });

    let api = TraceApi::from_events(events);

    Ok(ServerRuntime {
        api,
        event_store,
        lease_store,
        replay_store,
        guard,
    })
}

#[derive(Clone)]
struct ApiState {
    api: Arc<RwLock<TraceApi>>,
    event_store: EventStore,
    lease_store: LeaseIndexStore,
    replay_store: ReplayCheckpointStore,
    writer_lock: Arc<Mutex<()>>,
    tmux_script_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct CandidateQuery {
    include_disqualified: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskClaimRequest {
    worker_id: String,
    expected_epoch: Option<u64>,
    title: Option<String>,
    owner: Option<String>,
    reason: Option<String>,
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskLeaseUpdateRequest {
    worker_id: String,
    lease_epoch: u64,
    reason: Option<String>,
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunStartRequest {
    run_id: String,
    worker_id: String,
    lease_epoch: u64,
    model: Option<String>,
    provider: Option<String>,
    profile: Option<String>,
    temperature: Option<f64>,
    prompt_id: Option<String>,
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunOutputRequest {
    worker_id: String,
    lease_epoch: u64,
    stream: EventOutputStream,
    encoding: EventOutputEncoding,
    chunk: String,
    chunk_index: u64,
    #[serde(rename = "final", default)]
    final_chunk: bool,
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateCreateRequest {
    worker_id: String,
    lease_epoch: u64,
    candidate_id: Option<String>,
    stale: Option<bool>,
    reason: Option<String>,
    ts: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct BenchmarkEvaluateRequest {
    report_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct BenchmarkEvaluateResponse {
    report_id: String,
    json_report_path: String,
    markdown_report_path: String,
    summary: BenchmarkSummary,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TmuxStartRequest {
    session: Option<String>,
    trace_root: Option<String>,
    addr: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TmuxSessionRequest {
    session: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TmuxAddLaneRequest {
    session: Option<String>,
    lane_name: String,
    profile: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TmuxAddPaneRequest {
    session: Option<String>,
    lane_name: String,
    profile: Option<String>,
    target: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Serialize)]
struct TmuxCommandResponse {
    command: String,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    total_tasks: usize,
    total_runs: usize,
    total_events: usize,
    models: Vec<BenchmarkModelSummary>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    report_id: String,
    generated_at: String,
    total_events: usize,
    total_tasks: usize,
    total_runs: usize,
    models: Vec<BenchmarkModelSummary>,
    runs: Vec<BenchmarkRunSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkRunSummary {
    run_id: String,
    task_id: String,
    model: Option<String>,
    provider: Option<String>,
    profile: Option<String>,
    worker_id: Option<String>,
    lease_epoch: Option<u64>,
    started_at: Option<String>,
    completed_at: Option<String>,
    duration_ms: Option<i64>,
    candidate_total: usize,
    candidate_eligible: usize,
    candidate_disqualified: usize,
    output_chunks: usize,
    output_bytes: usize,
    verdict: Option<String>,
    passed: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkModelSummary {
    model_key: String,
    model: Option<String>,
    provider: Option<String>,
    profile: Option<String>,
    runs: usize,
    pass_count: usize,
    fail_count: usize,
    candidate_total: usize,
    candidate_eligible: usize,
    candidate_disqualified: usize,
    output_bytes: usize,
    avg_duration_ms: Option<f64>,
}

pub fn app_router(
    api: TraceApi,
    event_store: EventStore,
    lease_store: LeaseIndexStore,
    replay_store: ReplayCheckpointStore,
) -> Router {
    let tmux_script_path = resolve_tmux_script_path();
    app_router_with_tmux_script(
        api,
        event_store,
        lease_store,
        replay_store,
        tmux_script_path,
    )
}

fn app_router_with_tmux_script(
    api: TraceApi,
    event_store: EventStore,
    lease_store: LeaseIndexStore,
    replay_store: ReplayCheckpointStore,
    tmux_script_path: PathBuf,
) -> Router {
    let state = ApiState {
        api: Arc::new(RwLock::new(api)),
        event_store,
        lease_store,
        replay_store,
        writer_lock: Arc::new(Mutex::new(())),
        tmux_script_path,
    };

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
        .route("/events", post(post_event_handler))
        .route("/tasks/{task_id}/claim", post(post_task_claim_handler))
        .route("/tasks/{task_id}/renew", post(post_task_renew_handler))
        .route("/tasks/{task_id}/release", post(post_task_release_handler))
        .route(
            "/tasks/{task_id}/runs/start",
            post(post_run_started_handler),
        )
        .route(
            "/tasks/{task_id}/runs/{run_id}/output",
            post(post_run_output_handler),
        )
        .route(
            "/tasks/{task_id}/runs/{run_id}/candidates",
            post(post_candidate_create_handler),
        )
        .route(
            "/benchmarks/evaluate",
            post(post_benchmark_evaluate_handler),
        )
        .route("/orchestrator/tmux/start", post(post_tmux_start_handler))
        .route("/orchestrator/tmux/status", post(post_tmux_status_handler))
        .route(
            "/orchestrator/tmux/add-lane",
            post(post_tmux_add_lane_handler),
        )
        .route(
            "/orchestrator/tmux/add-pane",
            post(post_tmux_add_pane_handler),
        )
        .route("/orchestrator/tmux/stop", post(post_tmux_stop_handler))
        .with_state(state)
        .layer(cors_layer())
}

pub async fn serve(addr: SocketAddr, root: impl AsRef<Path>) -> Result<(), ServerError> {
    let runtime = bootstrap_runtime(root)?;
    runtime
        .assert_lease_sensitive_ready()
        .map_err(ServerError::Guard)?;

    let app = app_router(
        runtime.api,
        runtime.event_store,
        runtime.lease_store,
        runtime.replay_store,
    )
    .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn cors_layer() -> CorsLayer {
    let configured = std::env::var("TRACE_CORS_ALLOW_ORIGINS").ok();
    let mut origins = configured
        .as_deref()
        .map(parse_cors_origins)
        .unwrap_or_else(default_cors_origins);
    if origins.is_empty() {
        origins = default_cors_origins();
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any)
}

fn parse_cors_origins(raw: &str) -> Vec<HeaderValue> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| HeaderValue::from_str(value).ok())
        .collect()
}

fn default_cors_origins() -> Vec<HeaderValue> {
    DEFAULT_CORS_ALLOWED_ORIGINS
        .iter()
        .filter_map(|value| HeaderValue::from_str(value).ok())
        .collect()
}

fn resolve_tmux_script_path() -> PathBuf {
    std::env::var("TRACE_TMUX_ORCH_SCRIPT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_TMUX_SCRIPT_PATH))
}

fn validate_tmux_session(
    session: Option<String>,
) -> Result<String, (StatusCode, Json<ApiErrorResponse>)> {
    let session = session.unwrap_or_else(|| DEFAULT_TMUX_SESSION.to_string());
    validate_tmux_token("session", &session)?;
    Ok(session)
}

fn validate_tmux_token(
    field: &str,
    value: &str,
) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    if value.is_empty() {
        return Err(bad_request_error(format!("{field} cannot be empty")));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(bad_request_error(format!(
            "{field} contains invalid characters; allowed: [A-Za-z0-9._-]"
        )));
    }
    Ok(())
}

fn validate_tmux_target(value: &str) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    if value.is_empty() {
        return Err(bad_request_error("target cannot be empty"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '%'))
    {
        return Err(bad_request_error(
            "target contains invalid characters; allowed: [A-Za-z0-9._:-%]",
        ));
    }
    Ok(())
}

fn validate_tmux_lane_mode(value: &str) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    match value {
        "interactive" | "runner" => Ok(()),
        _ => Err(bad_request_error(
            "mode must be one of: interactive, runner",
        )),
    }
}

fn validate_trace_root(value: &str) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    if value.is_empty() {
        return Err(bad_request_error("trace_root cannot be empty"));
    }
    if value.contains('\0') || value.contains('\n') || value.contains('\r') {
        return Err(bad_request_error("trace_root contains invalid characters"));
    }
    Ok(())
}

fn validate_trace_server_addr(value: &str) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    value
        .parse::<SocketAddr>()
        .map(|_| ())
        .map_err(|_| bad_request_error("addr must be a valid socket address like 127.0.0.1:18080"))
}

async fn execute_tmux_script(
    state: &ApiState,
    args: Vec<String>,
) -> Result<Json<TmuxCommandResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let script_path = state.tmux_script_path.clone();
    let command = format!("{} {}", script_path.display(), args.join(" "));
    let command_args = args.clone();

    let output =
        tokio::task::spawn_blocking(move || Command::new(&script_path).args(command_args).output())
            .await
            .map_err(|error| internal_error(format!("failed to join tmux command task: {error}")))?
            .map_err(|error| internal_error(format!("failed to execute tmux command: {error}")))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let status = match exit_code {
            2 => StatusCode::BAD_REQUEST,
            1 => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let detail = if !stderr.is_empty() {
            stderr.clone()
        } else if !stdout.is_empty() {
            stdout.clone()
        } else {
            "tmux command failed with no output".to_string()
        };
        return Err((
            status,
            Json(ApiErrorResponse {
                error: format!("{command} exited with code {exit_code}: {detail}"),
            }),
        ));
    }

    Ok(Json(TmuxCommandResponse {
        command,
        exit_code,
        stdout,
        stderr,
    }))
}

async fn post_tmux_start_handler(
    State(state): State<ApiState>,
    Json(request): Json<TmuxStartRequest>,
) -> Result<Json<TmuxCommandResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let session = validate_tmux_session(request.session)?;
    let mut args = vec!["--session".to_string(), session];

    if let Some(trace_root) = request.trace_root {
        validate_trace_root(&trace_root)?;
        args.push("--trace-root".to_string());
        args.push(trace_root);
    }
    if let Some(addr) = request.addr {
        validate_trace_server_addr(&addr)?;
        args.push("--addr".to_string());
        args.push(addr);
    }

    args.push("start".to_string());
    args.push("--no-attach".to_string());

    execute_tmux_script(&state, args).await
}

async fn post_tmux_status_handler(
    State(state): State<ApiState>,
    Json(request): Json<TmuxSessionRequest>,
) -> Result<Json<TmuxCommandResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let session = validate_tmux_session(request.session)?;
    execute_tmux_script(
        &state,
        vec!["--session".to_string(), session, "status".to_string()],
    )
    .await
}

async fn post_tmux_add_lane_handler(
    State(state): State<ApiState>,
    Json(request): Json<TmuxAddLaneRequest>,
) -> Result<Json<TmuxCommandResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let TmuxAddLaneRequest {
        session,
        lane_name,
        profile,
        mode,
    } = request;

    let session = validate_tmux_session(session)?;
    validate_tmux_token("lane_name", &lane_name)?;
    let profile = profile.unwrap_or_else(|| lane_name.clone());
    validate_tmux_token("profile", &profile)?;

    let mut args = vec![
        "--session".to_string(),
        session,
        "add-lane".to_string(),
        lane_name,
        profile,
    ];
    if let Some(mode) = mode {
        validate_tmux_lane_mode(&mode)?;
        args.push(mode);
    }

    execute_tmux_script(&state, args).await
}

async fn post_tmux_add_pane_handler(
    State(state): State<ApiState>,
    Json(request): Json<TmuxAddPaneRequest>,
) -> Result<Json<TmuxCommandResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let TmuxAddPaneRequest {
        session,
        lane_name,
        profile,
        target,
        mode,
    } = request;

    let session = validate_tmux_session(session)?;
    validate_tmux_token("lane_name", &lane_name)?;
    let profile = profile.unwrap_or_else(|| lane_name.clone());
    validate_tmux_token("profile", &profile)?;

    let mut args = vec![
        "--session".to_string(),
        session,
        "add-pane".to_string(),
        lane_name,
        profile,
    ];
    if let Some(target) = target {
        validate_tmux_target(&target)?;
        args.push(target);
    }
    if let Some(mode) = mode {
        validate_tmux_lane_mode(&mode)?;
        args.push(mode);
    }

    execute_tmux_script(&state, args).await
}

async fn post_tmux_stop_handler(
    State(state): State<ApiState>,
    Json(request): Json<TmuxSessionRequest>,
) -> Result<Json<TmuxCommandResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let session = validate_tmux_session(request.session)?;
    execute_tmux_script(
        &state,
        vec!["--session".to_string(), session, "stop".to_string()],
    )
    .await
}

async fn get_tasks_handler(State(state): State<ApiState>) -> Json<Vec<TaskResponse>> {
    let api = state.api.read().expect("api lock should be readable");
    Json(api.get_tasks())
}

async fn get_task_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let api = state.api.read().expect("api lock should be readable");
    api.get_task(&task_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_task_timeline_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
) -> Json<Vec<TimelineEvent>> {
    let api = state.api.read().expect("api lock should be readable");
    Json(api.get_task_timeline(&task_id))
}

async fn get_run_timeline_handler(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
) -> Json<Vec<TimelineEvent>> {
    let api = state.api.read().expect("api lock should be readable");
    Json(api.get_run_timeline(&run_id))
}

async fn get_task_candidates_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
    Query(query): Query<CandidateQuery>,
) -> Json<Vec<CandidateSummary>> {
    let include_disqualified = query.include_disqualified.unwrap_or(false);
    let api = state.api.read().expect("api lock should be readable");
    Json(api.get_task_candidates(&task_id, include_disqualified))
}

async fn get_run_output_handler(
    State(state): State<ApiState>,
    AxumPath(run_id): AxumPath<String>,
) -> Json<Vec<RunOutputChunk>> {
    let api = state.api.read().expect("api lock should be readable");
    Json(api.get_run_output(&run_id))
}

#[derive(Debug, Serialize)]
struct ApiErrorResponse {
    error: String,
}

fn bad_request_error(message: impl Into<String>) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiErrorResponse {
            error: message.into(),
        }),
    )
}

fn conflict_error(message: impl Into<String>) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        StatusCode::CONFLICT,
        Json(ApiErrorResponse {
            error: message.into(),
        }),
    )
}

fn require_active_lease_holder_epoch(
    current_lease: Option<trace_lease::LeaseState>,
    payload: &Value,
    context: &str,
) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    let current_lease =
        current_lease.ok_or_else(|| conflict_error("lease is not currently claimed"))?;
    if !current_lease.active {
        return Err(conflict_error("lease is not currently claimed"));
    }

    let provided_holder = payload_string(
        payload,
        &["worker_id", "holder", "claimed_by", "released_by"],
    )
    .ok_or_else(|| bad_request_error(format!("{context} requires worker_id")))?;
    let expected_holder = current_lease
        .holder
        .unwrap_or_else(|| "unknown".to_string());
    if expected_holder != provided_holder {
        return Err(conflict_error(format!(
            "lease holder mismatch: expected={expected_holder}, provided={provided_holder}"
        )));
    }

    let provided_epoch = payload_u64(payload, &["lease_epoch", "epoch"])
        .ok_or_else(|| bad_request_error(format!("{context} requires lease_epoch")))?;
    if provided_epoch != current_lease.lease_epoch {
        return Err(conflict_error(format!(
            "stale lease epoch: provided={provided_epoch}, current={}",
            current_lease.lease_epoch
        )));
    }

    Ok(())
}

fn enforce_lease_fence(
    lease_store: &LeaseIndexStore,
    event: &NewTraceEvent,
) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    let current_lease = lease_store
        .current_lease(&event.task_id)
        .map_err(|error| internal_error(error.to_string()))?;

    match &event.kind {
        EventKind::TaskClaimed => {
            let current_epoch = current_lease
                .as_ref()
                .map(|lease| lease.lease_epoch)
                .unwrap_or(0);

            if let Some(provided_epoch) = payload_u64(&event.payload, &["lease_epoch", "epoch"]) {
                if provided_epoch < current_epoch {
                    return Err(conflict_error(format!(
                        "stale claim epoch: provided={provided_epoch}, current={current_epoch}"
                    )));
                }
            }

            if let Some(current_lease) = current_lease {
                if current_lease.active {
                    let holder = current_lease
                        .holder
                        .unwrap_or_else(|| "unknown".to_string());
                    return Err(conflict_error(format!(
                        "task already claimed by {holder} at epoch {}",
                        current_lease.lease_epoch
                    )));
                }
            }
        }
        EventKind::TaskRenewed => {
            require_active_lease_holder_epoch(current_lease, &event.payload, "task renewal")?;
        }
        EventKind::TaskReleased => {
            require_active_lease_holder_epoch(current_lease, &event.payload, "task release")?;
        }
        EventKind::RunStarted => {
            require_active_lease_holder_epoch(current_lease, &event.payload, "run start")?;
        }
        EventKind::RunnerOutput => {
            require_active_lease_holder_epoch(current_lease, &event.payload, "run output")?;
        }
        EventKind::ChangesetCreated => {
            let current_lease =
                current_lease.ok_or_else(|| conflict_error("lease is not currently claimed"))?;
            if !current_lease.active {
                return Err(conflict_error("lease is not currently claimed"));
            }

            let provided_epoch = payload_u64(&event.payload, &["lease_epoch", "epoch"])
                .ok_or_else(|| bad_request_error("changeset requires lease_epoch"))?;
            if provided_epoch != current_lease.lease_epoch {
                return Err(conflict_error(format!(
                    "stale candidate lease epoch: provided={provided_epoch}, current={}",
                    current_lease.lease_epoch
                )));
            }

            if let Some(provided_holder) =
                payload_string(&event.payload, &["worker_id", "holder", "claimed_by"])
            {
                let expected_holder = current_lease
                    .holder
                    .unwrap_or_else(|| "unknown".to_string());
                if provided_holder != expected_holder {
                    return Err(conflict_error(format!(
                        "lease holder mismatch: expected={expected_holder}, provided={provided_holder}"
                    )));
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn persist_event_locked(
    state: &ApiState,
    new_event: NewTraceEvent,
) -> Result<TraceEvent, (StatusCode, Json<ApiErrorResponse>)> {
    if new_event.global_seq.is_some() {
        return Err(bad_request_error(
            "new events must not include global_seq before persist",
        ));
    }

    enforce_lease_fence(&state.lease_store, &new_event)?;

    let persisted = state
        .event_store
        .append_event(new_event)
        .map_err(map_store_error)?;
    state
        .lease_store
        .apply_event(&persisted)
        .map_err(|error| internal_error(error.to_string()))?;
    state
        .replay_store
        .set_checkpoint_global_seq(persisted.global_seq)
        .map_err(|error| internal_error(error.to_string()))?;

    let refreshed = TraceApi::from_store(&state.event_store)
        .map_err(|error| internal_error(error.to_string()))?;
    {
        let mut api = state.api.write().expect("api lock should be writable");
        *api = refreshed;
    }

    Ok(persisted)
}

fn now_utc_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn event_ts_or_now(ts: Option<String>) -> String {
    ts.unwrap_or_else(now_utc_rfc3339)
}

fn maybe_insert_string(
    payload: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        payload.insert(key.to_string(), Value::String(value));
    }
}

fn trace_root_from_event_store(event_store: &EventStore) -> PathBuf {
    event_store
        .canonical_log_path()
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn sanitize_report_id(raw: &str) -> String {
    let sanitized: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "benchmark".to_string()
    } else {
        sanitized
    }
}

fn parse_rfc3339(ts: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(ts, &Rfc3339).ok()
}

fn duration_ms(started_at: Option<&str>, completed_at: Option<&str>) -> Option<i64> {
    let started = parse_rfc3339(started_at?)?;
    let completed = parse_rfc3339(completed_at?)?;
    let delta_ms = (completed - started).whole_milliseconds();
    if delta_ms < 0 {
        return None;
    }
    i64::try_from(delta_ms).ok()
}

fn infer_passed(payload: &Value) -> Option<bool> {
    if let Some(passed) = payload_bool(payload, &["pass", "passed", "success"]) {
        return Some(passed);
    }
    let verdict = payload_string(payload, &["verdict", "result", "outcome", "status"])?;
    match verdict.to_ascii_lowercase().as_str() {
        "pass" | "passed" | "ok" | "success" | "approved" => Some(true),
        "fail" | "failed" | "error" | "reject" | "rejected" => Some(false),
        _ => None,
    }
}

fn build_model_key(model: Option<&str>, provider: Option<&str>, profile: Option<&str>) -> String {
    let model = model.unwrap_or("unknown-model");
    let provider = provider.unwrap_or("unknown-provider");
    let profile = profile.unwrap_or("unknown-profile");
    format!("{provider}:{model}:{profile}")
}

fn build_benchmark_report(report_id: String, events: &[TraceEvent]) -> BenchmarkReport {
    let mut ordered = events.to_vec();
    ordered.sort_by_key(|event| event.global_seq);

    let api = TraceApi::from_events(ordered.clone());
    let mut runs: HashMap<String, BenchmarkRunSummary> = HashMap::new();

    for event in &ordered {
        let Some(run_id) = &event.run_id else {
            continue;
        };

        let entry = runs
            .entry(run_id.clone())
            .or_insert_with(|| BenchmarkRunSummary {
                run_id: run_id.clone(),
                task_id: event.task_id.clone(),
                model: None,
                provider: None,
                profile: None,
                worker_id: None,
                lease_epoch: None,
                started_at: None,
                completed_at: None,
                duration_ms: None,
                candidate_total: 0,
                candidate_eligible: 0,
                candidate_disqualified: 0,
                output_chunks: 0,
                output_bytes: 0,
                verdict: None,
                passed: None,
            });

        entry.task_id = event.task_id.clone();
        entry.completed_at = Some(event.ts.clone());

        match &event.kind {
            EventKind::RunStarted => {
                entry.started_at = Some(event.ts.clone());
                entry.model = payload_string(&event.payload, &["model", "model_name"]);
                entry.provider = payload_string(&event.payload, &["provider"]);
                entry.profile = payload_string(&event.payload, &["profile", "lane"]);
                entry.worker_id = payload_string(&event.payload, &["worker_id", "holder"]);
                entry.lease_epoch = payload_u64(&event.payload, &["lease_epoch", "epoch"]);
            }
            EventKind::RunnerOutput => {
                if let Ok(parsed) = validate_runner_output_payload(&event.payload) {
                    entry.output_chunks = entry.output_chunks.saturating_add(1);
                    entry.output_bytes = entry.output_bytes.saturating_add(parsed.chunk.len());
                }
            }
            EventKind::VerdictRecorded => {
                entry.verdict = payload_string(&event.payload, &["verdict", "result", "outcome"]);
                entry.passed = infer_passed(&event.payload);
            }
            _ => {}
        }
    }

    for task in api.get_tasks() {
        for candidate in api.get_task_candidates(&task.task.task_id, true) {
            let entry =
                runs.entry(candidate.run_id.clone())
                    .or_insert_with(|| BenchmarkRunSummary {
                        run_id: candidate.run_id.clone(),
                        task_id: candidate.task_id.clone(),
                        model: None,
                        provider: None,
                        profile: None,
                        worker_id: None,
                        lease_epoch: None,
                        started_at: None,
                        completed_at: None,
                        duration_ms: None,
                        candidate_total: 0,
                        candidate_eligible: 0,
                        candidate_disqualified: 0,
                        output_chunks: 0,
                        output_bytes: 0,
                        verdict: None,
                        passed: None,
                    });

            entry.task_id = candidate.task_id.clone();
            entry.candidate_total = entry.candidate_total.saturating_add(1);
            if candidate.eligible {
                entry.candidate_eligible = entry.candidate_eligible.saturating_add(1);
            } else {
                entry.candidate_disqualified = entry.candidate_disqualified.saturating_add(1);
            }
        }
    }

    for run in runs.values_mut() {
        run.duration_ms = duration_ms(run.started_at.as_deref(), run.completed_at.as_deref());
    }

    let mut model_map: HashMap<String, BenchmarkModelSummary> = HashMap::new();
    let mut model_duration_sums: HashMap<String, (i64, usize)> = HashMap::new();

    for run in runs.values() {
        let key = build_model_key(
            run.model.as_deref(),
            run.provider.as_deref(),
            run.profile.as_deref(),
        );

        let model = model_map
            .entry(key.clone())
            .or_insert_with(|| BenchmarkModelSummary {
                model_key: key.clone(),
                model: run.model.clone(),
                provider: run.provider.clone(),
                profile: run.profile.clone(),
                runs: 0,
                pass_count: 0,
                fail_count: 0,
                candidate_total: 0,
                candidate_eligible: 0,
                candidate_disqualified: 0,
                output_bytes: 0,
                avg_duration_ms: None,
            });

        model.runs = model.runs.saturating_add(1);
        model.candidate_total = model.candidate_total.saturating_add(run.candidate_total);
        model.candidate_eligible = model
            .candidate_eligible
            .saturating_add(run.candidate_eligible);
        model.candidate_disqualified = model
            .candidate_disqualified
            .saturating_add(run.candidate_disqualified);
        model.output_bytes = model.output_bytes.saturating_add(run.output_bytes);

        if run.passed == Some(true) {
            model.pass_count = model.pass_count.saturating_add(1);
        } else if run.passed == Some(false) {
            model.fail_count = model.fail_count.saturating_add(1);
        }

        if let Some(duration_ms) = run.duration_ms {
            let entry = model_duration_sums.entry(key).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(duration_ms);
            entry.1 = entry.1.saturating_add(1);
        }
    }

    let mut model_summaries = model_map.into_values().collect::<Vec<_>>();
    model_summaries.sort_by(|left, right| left.model_key.cmp(&right.model_key));
    for model in &mut model_summaries {
        if let Some((sum, count)) = model_duration_sums.get(&model.model_key) {
            if *count > 0 {
                model.avg_duration_ms = Some(*sum as f64 / *count as f64);
            }
        }
    }

    let mut run_summaries = runs.into_values().collect::<Vec<_>>();
    run_summaries.sort_by(|left, right| left.run_id.cmp(&right.run_id));

    BenchmarkReport {
        report_id,
        generated_at: now_utc_rfc3339(),
        total_events: ordered.len(),
        total_tasks: api.get_tasks().len(),
        total_runs: run_summaries.len(),
        models: model_summaries,
        runs: run_summaries,
    }
}

fn render_benchmark_markdown(report: &BenchmarkReport) -> String {
    let mut markdown = String::new();
    markdown.push_str(&format!(
        "# TRACE Benchmark Report: {}\n\n",
        report.report_id
    ));
    markdown.push_str(&format!("Generated at: {}\n\n", report.generated_at));
    markdown.push_str(&format!(
        "- Total events: {}\n- Total tasks: {}\n- Total runs: {}\n\n",
        report.total_events, report.total_tasks, report.total_runs
    ));

    markdown.push_str("## Model Summary\n\n");
    markdown.push_str("| Model Key | Runs | Pass | Fail | Eligible | Disqualified | Output Bytes | Avg Duration (ms) |\n");
    markdown.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for model in &report.models {
        let avg_duration = model
            .avg_duration_ms
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string());
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            model.model_key,
            model.runs,
            model.pass_count,
            model.fail_count,
            model.candidate_eligible,
            model.candidate_disqualified,
            model.output_bytes,
            avg_duration
        ));
    }

    markdown.push_str("\n## Run Summary\n\n");
    markdown.push_str("| Run ID | Task ID | Model | Candidates | Eligible | Disqualified | Verdict | Passed | Duration (ms) |\n");
    markdown.push_str("| --- | --- | --- | ---: | ---: | ---: | --- | --- | ---: |\n");
    for run in &report.runs {
        let model = build_model_key(
            run.model.as_deref(),
            run.provider.as_deref(),
            run.profile.as_deref(),
        );
        let verdict = run.verdict.clone().unwrap_or_else(|| "-".to_string());
        let passed = run
            .passed
            .map(|value| {
                if value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            })
            .unwrap_or_else(|| "-".to_string());
        let duration = run
            .duration_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            run.run_id,
            run.task_id,
            model,
            run.candidate_total,
            run.candidate_eligible,
            run.candidate_disqualified,
            verdict,
            passed,
            duration
        ));
    }

    markdown
}

async fn post_event_handler(
    State(state): State<ApiState>,
    Json(new_event): Json<NewTraceEvent>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;
    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_task_claim_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
    Json(request): Json<TaskClaimRequest>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;

    let current_lease = state
        .lease_store
        .current_lease(&task_id)
        .map_err(|error| internal_error(error.to_string()))?;
    let current_epoch = current_lease
        .as_ref()
        .map(|lease| lease.lease_epoch)
        .unwrap_or(0);

    if let Some(lease) = current_lease {
        if lease.active {
            let holder = lease.holder.unwrap_or_else(|| "unknown".to_string());
            return Err(conflict_error(format!(
                "task already claimed by {holder} at epoch {}",
                lease.lease_epoch
            )));
        }
    }

    if let Some(expected_epoch) = request.expected_epoch {
        if expected_epoch != current_epoch {
            return Err(conflict_error(format!(
                "claim epoch mismatch: expected={expected_epoch}, current={current_epoch}"
            )));
        }
    }

    let next_epoch = current_epoch.saturating_add(1);
    let mut payload = serde_json::Map::new();
    payload.insert("worker_id".to_string(), Value::String(request.worker_id));
    payload.insert("epoch".to_string(), Value::from(next_epoch));
    payload.insert("lease_epoch".to_string(), Value::from(next_epoch));
    maybe_insert_string(&mut payload, "title", request.title);
    maybe_insert_string(&mut payload, "owner", request.owner);
    maybe_insert_string(&mut payload, "reason", request.reason);

    let new_event = NewTraceEvent {
        global_seq: None,
        ts: event_ts_or_now(request.ts),
        task_id,
        run_id: None,
        kind: EventKind::TaskClaimed,
        payload: Value::Object(payload),
    };

    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_task_renew_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
    Json(request): Json<TaskLeaseUpdateRequest>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;

    let payload = serde_json::json!({
        "worker_id": request.worker_id,
        "epoch": request.lease_epoch,
        "lease_epoch": request.lease_epoch,
        "reason": request.reason,
    });

    let new_event = NewTraceEvent {
        global_seq: None,
        ts: event_ts_or_now(request.ts),
        task_id,
        run_id: None,
        kind: EventKind::TaskRenewed,
        payload,
    };

    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_task_release_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
    Json(request): Json<TaskLeaseUpdateRequest>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;

    let payload = serde_json::json!({
        "worker_id": request.worker_id,
        "released_by": request.worker_id,
        "epoch": request.lease_epoch,
        "lease_epoch": request.lease_epoch,
        "reason": request.reason,
    });

    let new_event = NewTraceEvent {
        global_seq: None,
        ts: event_ts_or_now(request.ts),
        task_id,
        run_id: None,
        kind: EventKind::TaskReleased,
        payload,
    };

    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_run_started_handler(
    State(state): State<ApiState>,
    AxumPath(task_id): AxumPath<String>,
    Json(request): Json<RunStartRequest>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;

    let mut payload = serde_json::Map::new();
    payload.insert("worker_id".to_string(), Value::String(request.worker_id));
    payload.insert("epoch".to_string(), Value::from(request.lease_epoch));
    payload.insert("lease_epoch".to_string(), Value::from(request.lease_epoch));
    maybe_insert_string(&mut payload, "model", request.model);
    maybe_insert_string(&mut payload, "provider", request.provider);
    maybe_insert_string(&mut payload, "profile", request.profile);
    maybe_insert_string(&mut payload, "prompt_id", request.prompt_id);
    if let Some(temperature) = request.temperature {
        if let Some(value) = serde_json::Number::from_f64(temperature) {
            payload.insert("temperature".to_string(), Value::Number(value));
        }
    }

    let new_event = NewTraceEvent {
        global_seq: None,
        ts: event_ts_or_now(request.ts),
        task_id,
        run_id: Some(request.run_id),
        kind: EventKind::RunStarted,
        payload: Value::Object(payload),
    };

    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_run_output_handler(
    State(state): State<ApiState>,
    AxumPath((task_id, run_id)): AxumPath<(String, String)>,
    Json(request): Json<RunOutputRequest>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;

    let payload = serde_json::json!({
        "worker_id": request.worker_id,
        "epoch": request.lease_epoch,
        "lease_epoch": request.lease_epoch,
        "stream": request.stream,
        "encoding": request.encoding,
        "chunk": request.chunk,
        "chunk_index": request.chunk_index,
        "final": request.final_chunk,
    });

    let new_event = NewTraceEvent {
        global_seq: None,
        ts: event_ts_or_now(request.ts),
        task_id,
        run_id: Some(run_id),
        kind: EventKind::RunnerOutput,
        payload,
    };

    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_candidate_create_handler(
    State(state): State<ApiState>,
    AxumPath((task_id, run_id)): AxumPath<(String, String)>,
    Json(request): Json<CandidateCreateRequest>,
) -> Result<(StatusCode, Json<TraceEvent>), (StatusCode, Json<ApiErrorResponse>)> {
    let _writer_guard = state.writer_lock.lock().await;

    let candidate_id = request
        .candidate_id
        .unwrap_or_else(|| format!("CAND-{task_id}-{run_id}-{}", request.lease_epoch));

    let mut payload = serde_json::Map::new();
    payload.insert("candidate_id".to_string(), Value::String(candidate_id));
    payload.insert("worker_id".to_string(), Value::String(request.worker_id));
    payload.insert("epoch".to_string(), Value::from(request.lease_epoch));
    payload.insert("lease_epoch".to_string(), Value::from(request.lease_epoch));
    if let Some(stale) = request.stale {
        payload.insert("stale".to_string(), Value::Bool(stale));
    }
    maybe_insert_string(&mut payload, "reason", request.reason);

    let new_event = NewTraceEvent {
        global_seq: None,
        ts: event_ts_or_now(request.ts),
        task_id,
        run_id: Some(run_id),
        kind: EventKind::ChangesetCreated,
        payload: Value::Object(payload),
    };

    let persisted = persist_event_locked(&state, new_event)?;
    Ok((StatusCode::CREATED, Json(persisted)))
}

async fn post_benchmark_evaluate_handler(
    State(state): State<ApiState>,
    Json(request): Json<BenchmarkEvaluateRequest>,
) -> Result<Json<BenchmarkEvaluateResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    let events = state
        .event_store
        .read_all_events()
        .map_err(|error| internal_error(error.to_string()))?;

    let default_report_id = format!(
        "bench-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let report_id = sanitize_report_id(request.report_id.as_deref().unwrap_or(&default_report_id));
    let report = build_benchmark_report(report_id.clone(), &events);

    let root = trace_root_from_event_store(&state.event_store);
    let reports_dir = root.join(".trace/reports");
    std::fs::create_dir_all(&reports_dir).map_err(|error| internal_error(error.to_string()))?;

    let json_report_path = reports_dir.join(format!("{report_id}.json"));
    let markdown_report_path = reports_dir.join(format!("{report_id}.md"));

    let json_report =
        serde_json::to_string_pretty(&report).map_err(|error| internal_error(error.to_string()))?;
    std::fs::write(&json_report_path, json_report)
        .map_err(|error| internal_error(error.to_string()))?;
    std::fs::write(&markdown_report_path, render_benchmark_markdown(&report))
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(BenchmarkEvaluateResponse {
        report_id,
        json_report_path: json_report_path.to_string_lossy().to_string(),
        markdown_report_path: markdown_report_path.to_string_lossy().to_string(),
        summary: BenchmarkSummary {
            total_tasks: report.total_tasks,
            total_runs: report.total_runs,
            total_events: report.total_events,
            models: report.models.clone(),
        },
    }))
}

fn map_store_error(error: std::io::Error) -> (StatusCode, Json<ApiErrorResponse>) {
    match error.kind() {
        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::InvalidData => {
            bad_request_error(error.to_string())
        }
        _ => internal_error(error.to_string()),
    }
}

fn internal_error(message: String) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiErrorResponse { error: message }),
    )
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use serde_json::{json, Value};
    use tower::util::ServiceExt;
    use trace_events::{EventKind, NewTraceEvent};
    use trace_lease::{LeaseIndexStore, ReplayCheckpointStore};
    use trace_store::EventStore;

    use super::{app_router_with_tmux_script, bootstrap_runtime, TraceApi, PHASE0_ENDPOINTS};

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

    fn build_test_app(root: &std::path::Path, store: &EventStore) -> axum::Router {
        build_test_app_with_tmux_script(root, store, PathBuf::from("scripts/trace-smoke-tmux.sh"))
    }

    fn build_test_app_with_tmux_script(
        root: &std::path::Path,
        store: &EventStore,
        tmux_script_path: PathBuf,
    ) -> axum::Router {
        let api = TraceApi::from_store(store).expect("projection should build");
        let lease_store = LeaseIndexStore::new(root).expect("lease store should initialize");
        let replay_store =
            ReplayCheckpointStore::new(root).expect("replay checkpoint store should initialize");

        let events = store
            .read_all_events()
            .expect("seeded events should be readable");
        lease_store
            .replay_events(&events)
            .expect("lease index should replay seeded events");
        replay_store
            .replay_to_tip(events.last().map(|event| event.global_seq).unwrap_or(0))
            .expect("checkpoint should advance to seeded tip");

        app_router_with_tmux_script(
            api,
            store.clone(),
            lease_store,
            replay_store,
            tmux_script_path,
        )
    }

    fn write_executable_script(path: &std::path::Path, content: &str) {
        fs::write(path, content).expect("script should be written");
        #[cfg(unix)]
        {
            let permissions = fs::Permissions::from_mode(0o755);
            fs::set_permissions(path, permissions).expect("script should be executable");
        }
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
        let app = build_test_app(&root, &store);

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
    async fn test_cors_simple_get_includes_allow_origin_for_local_dev() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks")
                    .method("GET")
                    .header("origin", "http://localhost:5173")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
            Some("http://localhost:5173")
        );

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_cors_preflight_allows_local_dev_origin() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks")
                    .method("OPTIONS")
                    .header("origin", "http://localhost:5173")
                    .header("access-control-request-method", "GET")
                    .header("access-control-request-headers", "content-type")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert!(response.status().is_success());
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|value| value.to_str().ok()),
            Some("http://localhost:5173")
        );
        let allow_methods = response
            .headers()
            .get("access-control-allow-methods")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(allow_methods.contains("GET"));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_tmux_start_route_invokes_configured_script_with_expected_args() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let args_log_path = root.join("tmux_args.log");
        let script_path = root.join("tmux-ok.sh");
        write_executable_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$*\" >> \"{}\"\necho \"started\"\n",
                args_log_path.display()
            ),
        );

        let app = build_test_app_with_tmux_script(&root, &store, script_path);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/orchestrator/tmux/start")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "session": "trace-web-test",
                            "trace_root": "/tmp/trace-web-test",
                            "addr": "127.0.0.1:18090"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should parse");
        assert_eq!(parsed["exit_code"].as_i64(), Some(0));

        let logged_args = fs::read_to_string(&args_log_path).expect("args log should be readable");
        assert!(logged_args.contains("--session trace-web-test"));
        assert!(logged_args.contains("--trace-root /tmp/trace-web-test"));
        assert!(logged_args.contains("--addr 127.0.0.1:18090"));
        assert!(logged_args.contains("start --no-attach"));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_tmux_add_lane_rejects_invalid_lane_name() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let script_path = root.join("tmux-unused.sh");
        write_executable_script(&script_path, "#!/usr/bin/env bash\nexit 0\n");
        let app = build_test_app_with_tmux_script(&root, &store, script_path);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/orchestrator/tmux/add-lane")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "session": "trace-web-test",
                            "lane_name": "bad lane",
                            "profile": "high"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_tmux_add_lane_passes_runner_mode_to_script() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let args_log_path = root.join("tmux_args.log");
        let script_path = root.join("tmux-ok.sh");
        write_executable_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$*\" >> \"{}\"\necho \"ok\"\n",
                args_log_path.display()
            ),
        );
        let app = build_test_app_with_tmux_script(&root, &store, script_path);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/orchestrator/tmux/add-lane")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "session": "trace-web-test",
                            "lane_name": "codex4",
                            "profile": "high",
                            "mode": "runner"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let logged_args = fs::read_to_string(&args_log_path).expect("args log should be readable");
        assert!(logged_args.contains("--session trace-web-test"));
        assert!(logged_args.contains("add-lane codex4 high runner"));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_tmux_add_pane_rejects_invalid_mode() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let script_path = root.join("tmux-unused.sh");
        write_executable_script(&script_path, "#!/usr/bin/env bash\nexit 0\n");
        let app = build_test_app_with_tmux_script(&root, &store, script_path);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/orchestrator/tmux/add-pane")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "session": "trace-web-test",
                            "lane_name": "codex5",
                            "profile": "flash",
                            "mode": "batch"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should parse");
        assert!(parsed["error"]
            .as_str()
            .unwrap_or_default()
            .contains("mode must be one of: interactive, runner"));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_tmux_status_maps_script_exit_code_one_to_conflict() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let script_path = root.join("tmux-fail.sh");
        write_executable_script(
            &script_path,
            "#!/usr/bin/env bash\nset -euo pipefail\necho \"session missing\" >&2\nexit 1\n",
        );
        let app = build_test_app_with_tmux_script(&root, &store, script_path);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/orchestrator/tmux/status")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "session": "missing-session"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should parse");
        assert!(parsed["error"]
            .as_str()
            .unwrap_or_default()
            .contains("session missing"));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_candidates_route_honors_query_toggle() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

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

    #[tokio::test]
    async fn test_post_events_appends_and_refreshes_projection() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let event_payload = json!({
            "global_seq": null,
            "ts": "2026-02-28T05:25:00.000Z",
            "task_id": "TASK-77",
            "run_id": null,
            "kind": "task.claimed",
            "payload": {
                "epoch": 1,
                "worker_id": "agent-9",
                "title": "New concurrent write task"
            }
        });

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(event_payload.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(create_response.status(), axum::http::StatusCode::CREATED);
        let create_body = to_bytes(create_response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let persisted: Value = serde_json::from_slice(&create_body).expect("response should parse");
        assert!(persisted.get("global_seq").is_some());

        let tasks_response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks")
                    .method("GET")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let tasks_body = to_bytes(tasks_response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let tasks: Vec<Value> = serde_json::from_slice(&tasks_body).expect("response should parse");

        assert!(tasks
            .iter()
            .any(|task| task["task"]["task_id"].as_str() == Some("TASK-77")));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_post_events_rejects_preassigned_global_seq() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let invalid_event = json!({
            "global_seq": 999,
            "ts": "2026-02-28T05:26:00.000Z",
            "task_id": "TASK-42",
            "run_id": null,
            "kind": "task.claimed",
            "payload": { "epoch": 8 }
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(invalid_event.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);

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

    #[tokio::test]
    async fn test_post_events_rejects_stale_claim_epoch() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let stale_claim = json!({
            "global_seq": null,
            "ts": "2026-02-28T05:27:00.000Z",
            "task_id": "TASK-42",
            "run_id": null,
            "kind": "task.claimed",
            "payload": { "epoch": 6, "worker_id": "agent-9" }
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(stale_claim.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_post_events_rejects_stale_candidate_epoch() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let stale_candidate = json!({
            "global_seq": null,
            "ts": "2026-02-28T05:28:00.000Z",
            "task_id": "TASK-42",
            "run_id": "RUN-21",
            "kind": "changeset.created",
            "payload": { "candidate_id": "C-200", "lease_epoch": 6 }
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(stale_candidate.to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_typed_claim_renew_release_flow() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let claim = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-100/claim")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-alpha",
                            "expected_epoch": 0,
                            "title": "Typed claim route",
                            "owner": "runtime"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(claim.status(), axum::http::StatusCode::CREATED);

        let renew = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-100/renew")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-alpha",
                            "lease_epoch": 1
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(renew.status(), axum::http::StatusCode::CREATED);

        let release = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-100/release")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-alpha",
                            "lease_epoch": 1
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(release.status(), axum::http::StatusCode::CREATED);

        let stale_claim = app
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-100/claim")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-beta",
                            "expected_epoch": 0
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(stale_claim.status(), axum::http::StatusCode::CONFLICT);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_typed_run_output_candidate_routes_enforce_lease() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let claim = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/claim")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-runner",
                            "expected_epoch": 0
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(claim.status(), axum::http::StatusCode::CREATED);

        let bad_run_start = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/runs/start")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "RUN-T200",
                            "worker_id": "wrong-holder",
                            "lease_epoch": 1,
                            "model": "gpt-5-high",
                            "provider": "openai",
                            "profile": "high"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(bad_run_start.status(), axum::http::StatusCode::CONFLICT);

        let run_start = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/runs/start")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "RUN-T200",
                            "worker_id": "agent-runner",
                            "lease_epoch": 1,
                            "model": "gpt-5-high",
                            "provider": "openai",
                            "profile": "high"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(run_start.status(), axum::http::StatusCode::CREATED);

        let stale_output = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/runs/RUN-T200/output")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-runner",
                            "lease_epoch": 0,
                            "stream": "stdout",
                            "encoding": "utf8",
                            "chunk": "oops",
                            "chunk_index": 0,
                            "final": false
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(stale_output.status(), axum::http::StatusCode::CONFLICT);

        let good_output = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/runs/RUN-T200/output")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-runner",
                            "lease_epoch": 1,
                            "stream": "stdout",
                            "encoding": "utf8",
                            "chunk": "ok",
                            "chunk_index": 1,
                            "final": true
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(good_output.status(), axum::http::StatusCode::CREATED);

        let bad_candidate = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/runs/RUN-T200/candidates")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "wrong-holder",
                            "lease_epoch": 1,
                            "candidate_id": "C-T200-1"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(bad_candidate.status(), axum::http::StatusCode::CONFLICT);

        let good_candidate = app
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-200/runs/RUN-T200/candidates")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-runner",
                            "lease_epoch": 1,
                            "candidate_id": "C-T200-2"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(good_candidate.status(), axum::http::StatusCode::CREATED);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_benchmark_evaluate_writes_json_and_markdown_reports() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let _claim = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-BENCH/claim")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-bench",
                            "expected_epoch": 0
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _run_start = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-BENCH/runs/start")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "RUN-BENCH-1",
                            "worker_id": "agent-bench",
                            "lease_epoch": 1,
                            "model": "gpt-5-flash",
                            "provider": "openai",
                            "profile": "flash"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _output = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-BENCH/runs/RUN-BENCH-1/output")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-bench",
                            "lease_epoch": 1,
                            "stream": "stdout",
                            "encoding": "utf8",
                            "chunk": "benchmark output",
                            "chunk_index": 0,
                            "final": true
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _candidate = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-BENCH/runs/RUN-BENCH-1/candidates")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-bench",
                            "lease_epoch": 1,
                            "candidate_id": "C-BENCH-1"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _verdict = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "global_seq": null,
                            "ts": "2026-02-28T05:35:00.000Z",
                            "task_id": "TASK-BENCH",
                            "run_id": "RUN-BENCH-1",
                            "kind": "verdict.recorded",
                            "payload": {
                                "verdict": "pass"
                            }
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/benchmarks/evaluate")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "report_id": "bench_test_report"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should parse");

        let json_report_path = parsed["json_report_path"]
            .as_str()
            .expect("json report path should exist");
        let markdown_report_path = parsed["markdown_report_path"]
            .as_str()
            .expect("markdown report path should exist");

        assert!(std::path::Path::new(json_report_path).exists());
        assert!(std::path::Path::new(markdown_report_path).exists());

        let report_json = fs::read_to_string(json_report_path).expect("report file should read");
        let report_value: Value =
            serde_json::from_str(&report_json).expect("report json should parse");
        assert!(report_value["total_runs"].as_u64().unwrap_or(0) >= 1);
        assert!(report_value["models"]
            .as_array()
            .map(|models| !models.is_empty())
            .unwrap_or(false));

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_typed_candidate_requires_active_lease() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-NO-LEASE/runs/RUN-X/candidates")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "agent-x",
                            "lease_epoch": 1,
                            "candidate_id": "C-NO-LEASE"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_benchmark_evaluate_aggregates_multi_model_pass_fail() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let _claim_flash = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-LANE-FLASH/claim")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "flash-worker",
                            "expected_epoch": 0
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _run_flash = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-LANE-FLASH/runs/start")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "RUN-FLASH-1",
                            "worker_id": "flash-worker",
                            "lease_epoch": 1,
                            "model": "gpt-5-flash",
                            "provider": "openai",
                            "profile": "flash"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _candidate_flash = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-LANE-FLASH/runs/RUN-FLASH-1/candidates")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "flash-worker",
                            "lease_epoch": 1,
                            "candidate_id": "C-FLASH-1"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _verdict_flash = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "global_seq": null,
                            "ts": "2026-02-28T06:10:00.000Z",
                            "task_id": "TASK-LANE-FLASH",
                            "run_id": "RUN-FLASH-1",
                            "kind": "verdict.recorded",
                            "payload": { "verdict": "pass" }
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _claim_high = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-LANE-HIGH/claim")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "high-worker",
                            "expected_epoch": 0
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _run_high = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-LANE-HIGH/runs/start")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "run_id": "RUN-HIGH-1",
                            "worker_id": "high-worker",
                            "lease_epoch": 1,
                            "model": "gpt-5-high",
                            "provider": "openai",
                            "profile": "high"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _candidate_high = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/TASK-LANE-HIGH/runs/RUN-HIGH-1/candidates")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "worker_id": "high-worker",
                            "lease_epoch": 1,
                            "candidate_id": "C-HIGH-1"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let _verdict_high = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/events")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "global_seq": null,
                            "ts": "2026-02-28T06:11:00.000Z",
                            "task_id": "TASK-LANE-HIGH",
                            "run_id": "RUN-HIGH-1",
                            "kind": "verdict.recorded",
                            "payload": { "verdict": "fail" }
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/benchmarks/evaluate")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "report_id": "bench_multi_lane"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should parse");
        let models = parsed["summary"]["models"]
            .as_array()
            .expect("summary.models should exist");

        let flash = models
            .iter()
            .find(|model| model["model_key"].as_str() == Some("openai:gpt-5-flash:flash"))
            .expect("flash model summary should exist");
        assert!(flash["pass_count"].as_u64().unwrap_or(0) >= 1);

        let high = models
            .iter()
            .find(|model| model["model_key"].as_str() == Some("openai:gpt-5-high:high"))
            .expect("high model summary should exist");
        assert!(high["fail_count"].as_u64().unwrap_or(0) >= 1);

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }

    #[tokio::test]
    async fn test_benchmark_report_id_is_sanitized_to_reports_directory() {
        let root = unique_temp_root();
        let store = seed_event_log(&root);
        let app = build_test_app(&root, &store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/benchmarks/evaluate")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "report_id": "../../escape/../name"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let parsed: Value = serde_json::from_slice(&body).expect("response should parse");
        let json_report_path = parsed["json_report_path"]
            .as_str()
            .expect("json report path should exist");
        let markdown_report_path = parsed["markdown_report_path"]
            .as_str()
            .expect("markdown report path should exist");

        assert!(!json_report_path.contains(".."));
        assert!(!markdown_report_path.contains(".."));
        assert!(json_report_path.contains(".trace/reports/"));
        assert!(markdown_report_path.contains(".trace/reports/"));
        assert!(std::path::Path::new(json_report_path).exists());
        assert!(std::path::Path::new(markdown_report_path).exists());

        fs::remove_dir_all(root).expect("cleanup should succeed");
    }
}
