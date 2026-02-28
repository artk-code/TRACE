# TRACE Agent Specification (Current)

Date: 2026-02-27  
Status: Active

## Objective
Build a working multi-agent evaluation system where multiple Codex terminals can run against the same TRACE server, produce competing candidates, and be benchmarked with deterministic scoring.

## Current Baseline
- Read API and projection pipeline exist from canonical event log.
- Event log path: `.trace/events/events.jsonl`.
- Replay checkpoint storage exists (`.trace/leases/index.sqlite3`).
- Web UI can read tasks/candidates/output from API.

## Phase 1 Priorities (Now)
1. Add write surface for agents.
  - `POST /events` (minimum) and typed writer paths for claim/renew/release/run/output/candidate.
2. Make append concurrency-safe.
  - Atomic sequence assignment under file lock.
  - Durability policy for append + fsync window.
3. Enforce lease authority from replayed state.
  - Replay canonical log into lease index and reject stale claims/writes.
4. Make projections live.
  - Update reads after writes without server restart.
5. Enable model-vs-model benchmarking.
  - Attach model metadata to runs/candidates.
  - Add benchmark/eval endpoints and report output.
6. Ship multi-terminal smoke flow.
  - Run several Codex terminals against one server and compare outcomes.

## Core Contracts
- Canonical persisted event shape:
  - `global_seq`, `ts`, `task_id`, `run_id?`, `kind`, `payload`
- `TaskResponse` remains nested:
  - `{ task, status, status_detail? }`
- Candidate default view excludes stale/disqualified.
- `runner.output` requires `encoding: utf8 | base64`.

## Phase 1 Exit Criteria
- Two or more terminals can write to one server concurrently without sequence corruption.
- Lease-sensitive operations reject stale epoch writes.
- Candidate compare and benchmark views show per-model outcomes.
- End-to-end smoke script runs and produces a benchmark report artifact.
