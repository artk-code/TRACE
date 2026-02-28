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

## Smoketest Readiness (2026-02-28)
- Multi-agent shared-server ingest (lock safety + lease fencing): **80%**
- Model-vs-model trace capture and reporting: **65%**
- TypeScript UI for benchmark compare workflows: **35%**
- Git merge + PR-compatible output from winning/stacked candidates: **15%**
- One-command real-user smoketest (Flash vs High vs Extra lanes): **25%**

## What Works Now
- Multiple writers can append concurrently without sequence corruption.
- Typed writer APIs exist for claim/renew/release/run/output/candidate.
- Lease-sensitive writes reject stale/mismatched holder+epoch paths.
- Run metadata fields (`model`, `provider`, `profile`, `temperature`) are accepted on run start.
- Benchmark reports can be generated from logged events into JSON+Markdown artifacts.
- tmux smoke orchestration scripts exist for human-in-the-loop multi-lane sessions:
  - `scripts/trace-smoke-tmux.sh`
  - `scripts/trace-lane-shell.sh`

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

## Active Priorities (Next Build)
1. Ship multi-terminal smoke harness.
  - Script end-to-end run for Flash/High/Extra profiles against one server.
  - Emit one benchmark report artifact per smoke run.
2. Add deterministic evaluator inputs.
  - Seed known task set with expected outcomes.
  - Score pass/fail + quality dimensions in report.
3. Add candidate artifact + merge contracts.
  - Extend `changeset.created` payload contract with code artifact references.
  - Define merge strategy output event(s) for winner/stacked merge.
4. Add git-compatible PR path.
  - Materialize winner/merged output to branch + commit metadata.
  - Produce machine-consumable PR payload/output (for `gh` or API handoff).
5. Expose benchmark + merge flows in UI/CLI.
  - Render model leaderboard and run breakdown from generated report files.
  - Add CLI commands for typed writes, benchmark generation, and merge/PR flow.
6. Add CI coverage for multi-writer + benchmark smoke.
  - Regression gate for typed writer routes and report generation path.
  - E2E smoke gate for multi-agent benchmark + merge output.

## Execution Plan (Super Smoketest Path)
1. P0: Automate lane simulation.
  - Add a repeatable script that runs Flash/High/Extra lanes against one shared TRACE root.
  - Emit one benchmark report and fail non-zero if report missing or malformed.
2. P1: Deterministic evaluator.
  - Add seeded tasks + expected outputs.
  - Compute pass/fail/quality from deterministic checks instead of event aggregation only.
3. P2: Merge and PR output.
  - Add candidate artifact contract (patch/diff references).
  - Add merge event flow and materialize git-compatible branch/commit metadata.
4. P3: UI + CLI workflow.
  - UI: benchmark leaderboard + run breakdown + merge action visibility.
  - CLI: typed write commands + benchmark + merge/PR output commands.

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
