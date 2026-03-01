# TRACE Agent Specification (Current)

Date: 2026-03-01  
Status: Active

## Objective
Build a multi-agent evaluation system where multiple Codex lanes run against one TRACE server, produce competing candidates, and are benchmarked through a browser-driven smoketest flow.

## Active Planning Docs
- `BUILD_SEQUENCE_PLAN_v3.md`
- `SMOKETEST_EVAL_PLAN_v2.md`
- `FRONTEND_PLAN_v5.md`

## Phase 0 QA Docs
- `docs/PHASE0_SIGNOFF.md`
- `docs/PHASE0_HUMAN_QA.md`
- `docs/LINUX_BUILD_HUMAN.md`

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
- Report retrieval endpoints exist:
  - `GET /reports`
  - `GET /reports/{report_id}`
  - list reads `.trace/reports/*.json` (latest-first, `limit` default `50`, max `200`)
  - get enforces strict `report_id` token validation (`[A-Za-z0-9_-]+`)
- Browser transport contract exists via CORS (dev origins + preflight coverage).
- Codex auth preflight endpoint exists:
  - `GET /orchestrator/auth/codex/status`
- Backend tmux orchestration endpoints exist:
  - `POST /orchestrator/tmux/start|status|add-lane|add-pane|stop`
- Backend smoke workflow endpoints exist:
  - `POST /smoke/runs`
  - `GET /smoke/runs/{run_id}`
  - workflow preflights tmux session+target and writes benchmark report on completion
- Backend lane-spawn auth enforcement exists:
  - `TRACE_CODEX_AUTH_POLICY=required|optional` (default: `required`)
  - `add-lane`/`add-pane` are blocked when auth is required and Codex is not logged in
- Smoke run history is bounded:
  - `TRACE_SMOKE_RUN_HISTORY_LIMIT` (default: `200`)
- Web UI can call tmux orchestration endpoints and display command results/errors.
- Web UI can trigger smoke workflow runs and poll smoke status from browser controls.
- Web UI can fetch/render latest benchmark report summaries from `/reports` APIs.
- Browser E2E smoke harness exists (Playwright) and CI runs `pnpm --dir web test:e2e`.

## Smoketest Readiness (2026-03-01)
- Shared-server ingest safety (lock + lease fencing): **82%**
- Model-vs-model trace capture/report generation: **78%**
- Web-driven orchestration control surface: **72%**
- Browser-driven smoke + report UX: **86%**
- Deterministic evaluator/scoring: **20%**
- Merge + PR-capable output pipeline: **15%**

## What Works Now
- Multiple writers can append without sequence corruption.
- Typed claim/run/output/candidate/release paths are active and fenced.
- tmux orchestration routes are active and validated for basic inputs.
- tmux add-lane/add-pane are auth-gated server-side when `TRACE_CODEX_AUTH_POLICY=required`.
- Smoke workflow API coordinates multi-lane runner launch, wait, and benchmark writeback.
- Report retrieval APIs expose benchmark JSON over HTTP for browser consumption.
- Web UI includes orchestration controls for start/status/add-lane/add-pane/stop and auth preflight status.
- Web UI includes smoke workflow controls for `Run Smoke` + `Refresh Status` with automatic active-run polling.
- Web UI includes `View Latest Report` with latest-report fetch and model summary table rendering.
- Playwright E2E covers auth check -> smoke run -> terminal status -> report visibility and is wired into CI.
- Lane shells support two execution modes:
  - `interactive` (manual copy/paste flow)
  - `runner` (scripted claim/run/output/candidate/verdict/release flow)
- Benchmark report generation writes JSON+Markdown artifacts with sanitized report IDs.

## Known Gaps Blocking Final Phase 0 Sign-Off
- Human QA evidence for a real tmux-backed browser smoke run is not yet recorded in `docs/PHASE0_SIGNOFF.md`.
- Automated Playwright E2E is API-stubbed by design for CI stability; live-environment validation is still a manual gate (`docs/PHASE0_HUMAN_QA.md`).

## Post-Phase0 Gaps
- Smoke workflow currently requires an existing tmux session and valid target (`session` + `target` preflight).
- Benchmark report is aggregation-oriented, not a deterministic quality evaluator.
- No seeded deterministic task/eval pack with expected-output contract.
- CLI remains read-oriented (`tasks`, `task`) and not smoke-run capable.
- No merge/PR pipeline from winning or stacked candidates.

## Active Priorities
1. Phase 0 human QA sign-off.
  - Run the manual browser/tmux smoke validation checklist.
  - Record run evidence and verdict in `docs/PHASE0_SIGNOFF.md`.
2. Deterministic eval contract.
  - Add seeded task pack + expected-output scoring contract.
  - Make benchmark quality signals reproducible across reruns.
3. Merge/PR pipeline (after smoke stability).
  - Add winning/stacked candidate export and Git-compatible PR path.

## Immediate Build Sequence
1. Complete manual Phase 0 QA run and write sign-off record.
2. Add deterministic seeded eval pack.
3. Add merge/PR workflow after smoke is reliable.

## Tmux Orchestration Runbook (Now)
Prerequisite:
- `tmux` installed on host running TRACE operator terminals.

1. Start session (server + flash/high/extra + observer):
  - `scripts/trace-smoke-tmux.sh start`
2. Attach from terminal:
  - `scripts/trace-smoke-tmux.sh attach`
3. Check Codex auth status:
  - `curl -sS http://127.0.0.1:18086/orchestrator/auth/codex/status | jq .`
  - if policy is `required`, run login if `logged_in=false`:
    - `codex login`
    - `codex login --device-auth`
4. Add lane window:
  - `scripts/trace-smoke-tmux.sh add-lane codex4 high`
  - `scripts/trace-smoke-tmux.sh add-lane codex4 high runner`
5. Add lane pane:
  - `scripts/trace-smoke-tmux.sh add-pane codex5 flash trace-smoke:lanes`
  - `scripts/trace-smoke-tmux.sh add-pane codex5 flash trace-smoke:lanes runner`
6. Runner knobs (optional):
  - `TRACE_RUNNER_TASK_COUNT=3`
  - `TRACE_RUNNER_TASK_PREFIX=TASK-SMOKE`
  - `TRACE_RUNNER_VERDICT=pass`
  - `TRACE_RUNNER_EXIT_AFTER_RUN=1`
7. Check status:
  - `scripts/trace-smoke-tmux.sh status`
8. Validate target (before smoke workflow triggers):
  - `scripts/trace-smoke-tmux.sh validate-target trace-smoke:lanes`
9. Stop:
  - `scripts/trace-smoke-tmux.sh stop`

## Tmux Bug Ledger (Current)
- Fixed: `add-lane`/`add-pane` inherit `TRACE_ROOT` + `TRACE_SERVER_ADDR` from session env when global flags omitted.
- Fixed: `status` pane listing is session-scoped.
- Fixed: server pane startup falls back to `cargo run -p trace-server` when `rustup stable` fails/unavailable.
- Fixed: lane runner mode (`mode=runner`) now emits typed write events plus `verdict.recorded` without manual copy/paste.
- Fixed: `wait-lane` now matches pane metadata (`@trace_lane_name`) with robust delimiter parsing; smoke runner lanes no longer false-timeout.
- Fixed: smoke workflow preflights tmux session/target before enqueue, failing fast instead of failing later in runner spawn.
- Fixed: smoke benchmark event scoping now filters by lane identity, preventing unrelated events from contaminating reports.
- Fixed: smoke run in-memory history is bounded with pruning (`TRACE_SMOKE_RUN_HISTORY_LIMIT`).
- Fixed: web smoke polling now keeps retrying after transient `GET /smoke/runs/{run_id}` failures.
- Fixed: web `View Latest Report` flow fetches benchmark reports via `/reports` and renders model summary table.
- Fixed: Playwright browser smoke (`auth -> run smoke -> report visible`) is now CI-gated.
- Open: pane command injection can race if commands are blasted without pacing.
- Open: no autonomous lane lifecycle manager yet.

## Orchestration Pitfalls
- Run exactly one TRACE server process per shared `TRACE_ROOT`.
- Keep `run_id` globally unique (not just per task).
- Use wrapper scripts for pane/window creation instead of raw tmux command strings.
- Use `mode=runner` when you want scripted writes; default lane mode remains interactive.
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
- `test_tmux_add_lane_requires_codex_auth_when_policy_required`
- `test_tmux_add_pane_allows_when_policy_optional_and_not_logged_in`
- `test_tmux_add_lane_passes_runner_mode_to_script`
- `test_codex_auth_status_reports_chatgpt_login`
- `test_codex_auth_status_reports_missing_binary`
- `test_tmux_add_pane_rejects_invalid_mode`
- `test_tmux_status_maps_script_exit_code_one_to_conflict`
- `test_smoke_run_rejects_when_tmux_session_preflight_fails`
- `test_smoke_run_rejects_when_tmux_target_preflight_fails`
- `test_smoke_run_benchmark_scopes_out_unrelated_events_after_start`
- `test_smoke_run_history_limit_prunes_old_terminal_runs`
- `test_get_reports_returns_empty_when_reports_directory_missing`
- `test_get_reports_lists_only_json_and_sorts_latest_first`
- `test_get_report_rejects_invalid_report_id`
- `test_get_report_rejects_path_traversal_tokens`
- `test_get_report_returns_not_found_for_missing_report`
- `test_get_report_returns_json_payload_for_existing_report`
- `web/src/guards.test.ts` runtime schema guard coverage
- `web/tests/phase0-smoke.spec.ts` browser E2E smoke (Playwright)

## Exit Criteria
- Browser UI can trigger and observe a full multi-lane smoke run.
- Smoke run emits concurrent writes without `global_seq` corruption.
- Benchmark results are retrievable/renderable in browser.
- Browser E2E smoke is stable and CI-gated.
