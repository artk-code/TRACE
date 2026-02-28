# TRACE Build Sequence Plan v2

Date: 2026-02-27  
Depends on: `AGENTS.md`

## Goal
Move from read-focused Phase 0 scaffold to a usable multi-terminal benchmark system.

## Sequence
1. Writer API + ingest validation.
  - Implement `POST /events` and schema validation for accepted event kinds.
  - Add server tests for accepted/rejected payloads.
2. Concurrency-safe append.
  - Replace read-max-then-append race with atomic append and monotonic seq guarantees.
  - Add stress test for concurrent writers.
3. Lease replay + fencing enforcement.
  - Replay log to lease state on startup and incrementally on writes.
  - Reject stale claim and stale candidate writes.
4. Live projection refresh.
  - Keep projection view in sync after each successful write.
  - Ensure GET routes reflect fresh data without restart.
5. Benchmark/eval plumbing.
  - Add run metadata (`model`, `provider`, `temperature`, etc.).
  - Add benchmark endpoint/report generation for cross-model comparisons.
6. Smoke UX + CLI flow.
  - Add commands/scripts to seed task, run multiple agents, collect outputs, compare winners.
7. CI expansion.
  - Add integration/E2E jobs for multi-writer and benchmark smoke.

## Blocking Risks
- Sequence collisions under parallel writes.
- Lease replay lag allowing stale updates.
- Projection staleness after writes.
- Missing benchmark provenance per run/candidate.

## Required Regression Gates
- `rustup run stable cargo fmt --all --check`
- `rustup run stable cargo clippy --workspace --all-targets -- -D warnings`
- `rustup run stable cargo test --workspace`
- `pnpm --dir web test`
- `pnpm --dir web build`
