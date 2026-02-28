# TRACE Agent Specification (Current)

Date: 2026-02-28  
Status: Active

## Objective
Build a multi-agent evaluation system where multiple Codex lanes run against one TRACE server, produce competing candidates, and are benchmarked through a browser-driven smoketest flow.

## Active Planning Docs
- `BUILD_SEQUENCE_PLAN_v3.md`
- `SMOKETEST_EVAL_PLAN_v2.md`
- `FRONTEND_PLAN_v5.md`

## Current Baseline
- Canonical log + projection pipeline is active.
- Write APIs exist on both generic and typed paths:
  - `POST /events`
  - `POST /tasks/{task_id}/claim|renew|release`
  - `POST /tasks/{task_id}/runs/start`
  - `POST /tasks/{task_id}/runs/{run_id}/output`
  - `POST /tasks/{task_id}/runs/{run_id}/candidates`
- Concurrent append is lock-safe with monotonic `global_seq`.
- Replay checkpoint storage exists (`.trace/leases/index.sqlite3`).
- Server startup replays event log into lease index before lease-sensitive operations.
- Lease-sensitive write paths reject stale or holder-mismatched epochs.
- Benchmark endpoint exists:
  - `POST /benchmarks/evaluate`
  - Writes `.trace/reports/<report_id>.json` + `.md`
- Browser transport contract exists via CORS (dev origins + preflight coverage).
- Backend tmux orchestration endpoints exist:
  - `POST /orchestrator/tmux/start|status|add-lane|add-pane|stop`
- Web UI can call tmux orchestration endpoints and display command results/errors.

## Smoketest Readiness (2026-02-28)
- Shared-server ingest safety (lock + lease fencing): **80%**
- Model-vs-model trace capture/report generation: **70%**
- Web-driven orchestration control surface: **60%**
- Browser-driven smoke + report UX: **45%**
- Deterministic evaluator/scoring: **20%**
- Merge + PR-capable output pipeline: **15%**

## What Works Now
- Multiple writers can append without sequence corruption.
- Typed claim/run/output/candidate/release paths are active and fenced.
- tmux orchestration routes are active and validated for basic inputs.
- Web UI includes orchestration controls for start/status/add-lane/add-pane/stop.
- Benchmark report generation writes JSON+Markdown artifacts with sanitized report IDs.

## Known Gaps Blocking "Super Smoketest"
- Lane execution is still manual/human-in-the-loop:
  - panes open interactive shells with copy/paste hints.
- No smoke workflow endpoint coordinating scripted Flash/High/Extra runs.
- No report list/get API for browser retrieval; reports are filesystem artifacts only.
- Benchmark report is aggregation-oriented, not a deterministic quality evaluator.
- No seeded deterministic task/eval pack with expected-output contract.
- No browser E2E harness (Playwright) gating orchestration/report flows.
- CLI remains read-oriented (`tasks`, `task`) and not smoke-run capable.
- No merge/PR pipeline from winning or stacked candidates.

## Active Priorities
1. Scripted lane runner mode.
  - Keep interactive tmux mode for humans.
  - Add non-interactive lane execution path for smoke automation.
2. Smoke workflow API.
  - One trigger to coordinate multi-lane run lifecycle.
  - Return status for polling in web UI.
3. Report retrieval APIs.
  - Add report list/get endpoints rooted under `.trace/reports`.
4. Web smoke/report UX.
  - Add run/evaluate/report display surfaces in web app.
5. Browser E2E + CI gate.
  - Add Playwright smoke test and enforce in CI.
6. Deterministic evaluator + merge pipeline.
  - Seed expected-output tasks.
  - Add scoring contract and merge/PR output path.

## Tmux Orchestration Runbook (Now)
Prerequisite:
- `tmux` installed on host running TRACE operator terminals.

1. Start session (server + flash/high/extra + observer):
  - `scripts/trace-smoke-tmux.sh start`
2. Attach from terminal:
  - `scripts/trace-smoke-tmux.sh attach`
3. Add lane window:
  - `scripts/trace-smoke-tmux.sh add-lane codex4 high`
4. Add lane pane:
  - `scripts/trace-smoke-tmux.sh add-pane codex5 flash trace-smoke:lanes`
5. Check status:
  - `scripts/trace-smoke-tmux.sh status`
6. Stop:
  - `scripts/trace-smoke-tmux.sh stop`

## Tmux Bug Ledger (Current)
- Fixed: `add-lane`/`add-pane` inherit `TRACE_ROOT` + `TRACE_SERVER_ADDR` from session env when global flags omitted.
- Fixed: `status` pane listing is session-scoped.
- Fixed: server pane startup falls back to `cargo run -p trace-server` when `rustup stable` fails/unavailable.
- Open: pane command injection can race if commands are blasted without pacing.
- Open: no autonomous lane lifecycle manager yet.

## Orchestration Pitfalls
- Run exactly one TRACE server process per shared `TRACE_ROOT`.
- Keep `run_id` globally unique (not just per task).
- Use wrapper scripts for pane/window creation instead of raw tmux command strings.
- Treat lane panes as human shells until scripted runner mode lands.
- Do not treat benchmark pass/fail as authoritative quality until deterministic evaluator lands.

## Core Contracts
- Persisted event shape:
  - `global_seq`, `ts`, `task_id`, `run_id?`, `kind`, `payload`
- `TaskResponse` shape remains nested:
  - `{ task, status, status_detail? }`
- Candidate default view excludes stale/disqualified unless requested.
- `runner.output` requires `encoding: utf8 | base64`.
- Benchmark reports are written under `.trace/reports/`.

## Sanity Test Matrix (Current)
- `test_typed_claim_renew_release_flow`
- `test_typed_run_output_candidate_routes_enforce_lease`
- `test_typed_candidate_requires_active_lease`
- `test_benchmark_evaluate_writes_json_and_markdown_reports`
- `test_benchmark_evaluate_aggregates_multi_model_pass_fail`
- `test_benchmark_report_id_is_sanitized_to_reports_directory`
- `test_cors_simple_get_includes_allow_origin_for_local_dev`
- `test_cors_preflight_allows_local_dev_origin`
- `test_tmux_start_route_invokes_configured_script_with_expected_args`
- `test_tmux_add_lane_rejects_invalid_lane_name`
- `test_tmux_status_maps_script_exit_code_one_to_conflict`
- `web/src/guards.test.ts` runtime schema guard coverage

## Exit Criteria
- Browser UI can trigger and observe a full multi-lane smoke run.
- Smoke run emits concurrent writes without `global_seq` corruption.
- Benchmark results are retrievable/renderable in browser.
- Browser E2E smoke is stable and CI-gated.
