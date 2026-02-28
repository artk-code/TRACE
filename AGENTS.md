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

## What Is Still Broken For The Super Smoketest
- No orchestration harness to launch and coordinate multiple Codex agents against one server.
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
