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

## Active Priorities (Next Build)
1. Ship multi-terminal smoke harness.
  - Script end-to-end run for Flash/High/Extra profiles against one server.
  - Emit one benchmark report artifact per smoke run.
2. Add deterministic evaluator inputs.
  - Seed known task set with expected outcomes.
  - Score pass/fail + quality dimensions in report.
3. Expand benchmark provenance.
  - Normalize run metadata fields (`model`, `provider`, `profile`, `temperature`, prompt/build ids).
  - Include latency/completion and stale-disqualification counters in report summary.
4. Expose benchmark views in UI/CLI.
  - Render model leaderboard and run breakdown from generated report files.
5. Add CI coverage for multi-writer + benchmark smoke.
  - Regression gate for typed writer routes and report generation path.

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
