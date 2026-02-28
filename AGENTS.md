# TRACE Agent Specification (Current)

Date: 2026-02-28  
Status: Active

## Objective
Build a working multi-agent evaluation system where multiple Codex terminals can run against the same TRACE server, produce competing candidates, and be benchmarked with deterministic scoring.

## Current Baseline
- Read API and projection pipeline exist from canonical event log.
- Write API exists on both generic and typed paths:
  - `POST /events`
  - `POST /tasks/{task_id}/claim|renew|release`
  - `POST /tasks/{task_id}/runs/start`
  - `POST /tasks/{task_id}/runs/{run_id}/output`
  - `POST /tasks/{task_id}/runs/{run_id}/candidates`
- Event log path: `.trace/events/events.jsonl`.
- Concurrent append uses file lock (`.trace/events/events.lock`) with monotonic `global_seq`.
- Replay checkpoint storage exists (`.trace/leases/index.sqlite3`).
- Server boot replays canonical log into lease index before opening lease-sensitive paths.
- Lease-sensitive writes reject stale or mismatched holder/epoch on claim/renew/release/run/output/candidate paths.
- Benchmark report endpoint exists:
  - `POST /benchmarks/evaluate` writes `.trace/reports/<report_id>.json` and `.trace/reports/<report_id>.md`.
- Web UI can read tasks/candidates/output from API.
- Web/API browser transport contract exists via CORS policy (dev origins + preflight handling).
- Backend orchestration control-plane endpoints exist:
  - `POST /orchestrator/tmux/start|status|add-lane|add-pane|stop`

## Smoketest Readiness (2026-02-28)
- Multi-agent shared-server ingest (lock safety + lease fencing): **80%**
- Model-vs-model trace capture and reporting: **65%**
- TypeScript UI for benchmark compare workflows: **40%**
- Git merge + PR-compatible output from winning/stacked candidates: **15%**
- One-command real-user smoketest (Flash vs High vs Extra lanes): **45%**

## What Works Now
- Multiple writers can append concurrently without sequence corruption.
- Typed writer APIs exist for claim/renew/release/run/output/candidate.
- Lease-sensitive writes reject stale/mismatched holder+epoch paths.
- Run metadata fields (`model`, `provider`, `profile`, `temperature`) are accepted on run start.
- Benchmark reports can be generated from logged events into JSON+Markdown artifacts.
- tmux smoke orchestration scripts exist for human-in-the-loop multi-lane sessions:
  - `scripts/trace-smoke-tmux.sh`
  - `scripts/trace-lane-shell.sh`
- Web transport from browser origins is enabled and tested via CORS.
- Backend tmux orchestration routes are implemented and tested.

## What Is Still Broken For The Super Smoketest
- Orchestration is manual/human-in-the-loop; no autonomous Codex runner lifecycle yet.
- No seeded deterministic evaluator task pack with expected-output scoring rules.
- Benchmark report is aggregation-only today; it is not yet a deterministic judge for quality.
- No persisted code artifact linkage per candidate (patch/diff metadata is not modeled yet).
- No merge engine for combining multiple candidate outputs into one merged changeset.
- No git branch/merge/PR pipeline from winner or merged candidates.
- Web UI is read-only for traces; it does not yet drive benchmark runs or render leaderboard/report UX.
- CLI is read-only; it does not expose typed writer or benchmark commands.
- CI does not yet run an end-to-end multi-agent smoke benchmark.

## Web-Orchestration Blockers (Confirmed 2026-02-28, Post-Step-2)
- Resolved: web transport contract exists via server CORS policy (local dev origins + preflight coverage).
- Resolved: backend orchestration control-plane route surface exists for tmux lifecycle.
- Web UI is still read-only for orchestration:
  - no POST actions for tmux orchestration commands.
  - no write-path actions for claim/run/output/candidate from UI workflows.
- Lane execution path is still manual:
  - lane panes open interactive shells and rely on copy/paste operator commands.
- Benchmark artifacts are filesystem-oriented:
  - `POST /benchmarks/evaluate` writes files and returns local paths, but no report retrieval/list route exists for the UI.
- Deterministic evaluator dataset/scoring contract is still absent.
- CI lacks end-to-end web-driven smoke coverage.

## Active Priorities (Web Smoke Path)
1. Add web orchestration actions.
  - Add API client + UI controls for tmux session lifecycle operations.
  - Surface status/errors clearly to operator.
2. Add non-interactive lane runner mode.
  - Keep current interactive mode for humans.
  - Add deterministic scripted mode for web-triggered smoke runs.
3. Add benchmark report retrieval routes.
  - List report ids and fetch report JSON/markdown by id.
  - Keep path sanitization and root scoping guarantees.
4. Add deterministic evaluator inputs.
  - Seed known tasks and expected outputs.
  - Move report scoring beyond aggregation-only.
5. Add CI E2E smoke gate.
  - Verify web-triggered orchestration produces event log + benchmark artifact.

## Execution Plan (Web Smoke Control Plane v1)
1. P0: Connectivity + control-plane skeleton.
  - Land web/API connectivity contract (proxy or CORS). ✅
  - Land orchestration routes that wrap existing tmux scripts. ✅
2. P1: Automated lane execution.
  - Add scripted lane mode that performs claim/run/output/candidate/release.
  - Add smoke endpoint/workflow that launches Flash/High/Extra lanes.
3. P2: Report retrieval + web workflow.
  - Add report list/get routes and wire UI to trigger/read smoke reports.
  - Show model summary table for smoke runs.
4. P3: Deterministic evaluator + CI.
  - Add seeded evaluator task pack and scoring rules.
  - Add E2E CI gate for web-orchestrated smoke.
5. P4: Merge/PR pipeline.
  - Add candidate artifact references, merge strategy events, and git/PR output path.

## Tmux Orchestration Runbook (Now)
Prerequisite:
- `tmux` must be installed on the host running TRACE/operator terminals.

1. Start session (server + flash/high/extra + observer):
  - `scripts/trace-smoke-tmux.sh start`
2. Attach from any terminal:
  - `scripts/trace-smoke-tmux.sh attach`
3. Add a new lane as a window:
  - `scripts/trace-smoke-tmux.sh add-lane codex4 high`
4. Add a new lane as a pane (for web-triggered terminal spawn behavior):
  - `scripts/trace-smoke-tmux.sh add-pane codex5 flash trace-smoke:lanes`
5. Session introspection:
  - `scripts/trace-smoke-tmux.sh status`
6. Stop session:
  - `scripts/trace-smoke-tmux.sh stop`

## Tmux Bug Ledger (2026-02-28)
- Fixed: `add-lane`/`add-pane` now hydrate `TRACE_ROOT` + `TRACE_SERVER_ADDR` from tmux session env when global flags are omitted.
  - Impact: dynamic lane spawns no longer drift to default `127.0.0.1:18080` or repo-local `.trace-smoke`.
- Fixed: `status` pane listing is now session-scoped.
  - Impact: `scripts/trace-smoke-tmux.sh status` no longer mixes panes from unrelated tmux sessions.
- Fixed: server pane startup now falls back to `cargo run -p trace-server` when `rustup stable` is unavailable/fails.
  - Impact: fewer false-negative startup failures on partially configured hosts.
- Open: pane command injection via `tmux send-keys` can race if multiple commands are blasted without pacing.
  - Mitigation: send one command at a time and wait for prompt/API response between steps.
- Open: orchestration remains human-driven; there is still no autonomous runner lifecycle manager.

## Orchestration Pitfalls (Important)
- Run exactly one TRACE server process per shared `TRACE_ROOT`.
- Keep `run_id` globally unique (not just per task) to avoid run aggregation collisions.
- Use the wrapper script for pane/window creation instead of raw tmux command strings.
- Treat lane panes as human shells: commands are operator-driven unless an explicit runner is added.
- If spawning lanes from a web backend, call the wrapper script with validated lane/profile values.
- Do not rely on benchmark pass/fail as authoritative quality scoring until deterministic evaluator logic lands.

## Quick Verification (Tmux)
1. `scripts/trace-smoke-tmux.sh --session trace-smoke-check --trace-root /tmp/trace-smoke-check --addr 127.0.0.1:18086 start --no-attach`
2. `scripts/trace-smoke-tmux.sh --session trace-smoke-check status`
  - Verify `session config` shows `/tmp/trace-smoke-check` and `127.0.0.1:18086`.
3. `scripts/trace-smoke-tmux.sh --session trace-smoke-check add-lane codex4 high`
4. `scripts/trace-smoke-tmux.sh --session trace-smoke-check add-pane codex5 flash trace-smoke-check:lanes`
5. `curl -sS http://127.0.0.1:18086/tasks`
  - Expect JSON response (typically `[]` before writes).
6. `curl -sS -X POST http://127.0.0.1:18086/benchmarks/evaluate -H 'content-type: application/json' -d '{"report_id":"tmux_smoke_manual"}'`
7. `scripts/trace-smoke-tmux.sh --session trace-smoke-check stop`

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

## Core Contracts
- Canonical persisted event shape:
  - `global_seq`, `ts`, `task_id`, `run_id?`, `kind`, `payload`
- `TaskResponse` remains nested:
  - `{ task, status, status_detail? }`
- Candidate default view excludes stale/disqualified.
- `runner.output` requires `encoding: utf8 | base64`.
- Benchmark report artifacts are written under `.trace/reports/`.

## Exit Criteria
- Two or more terminals can write to one server concurrently without sequence corruption.
- Lease-sensitive operations reject stale epoch writes.
- Candidate compare and benchmark views show per-model outcomes.
- End-to-end smoke script runs and produces a benchmark report artifact.
